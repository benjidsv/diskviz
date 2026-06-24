//! Tauri command surface. The scanned tree lives here in managed state; the
//! frontend pulls only the bounded slice it renders via `get_subtree`, so IPC
//! payloads stay tiny regardless of how many millions of files were scanned.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::scanner::{self, Progress, ScanTree};

#[derive(Default)]
pub struct AppState {
    pub tree: Mutex<Option<ScanTree>>,
    /// Set by `cancel_scan` and polled inside the active scan to abort early.
    pub cancel: Arc<AtomicBool>,
}

/// Mirrors vizdisk's `FileNode` shape so the ported React components work
/// unchanged — except `children` is only filled to the requested depth.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileNodeDto {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub size: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub children: Vec<FileNodeDto>,
    /// Immediate children that were truncated away (beyond `max_children`).
    pub hidden_children: u64,
    /// Summed size of those truncated children — lets the UI show an honest
    /// "Other" bucket without re-deriving it from the node's own total.
    pub hidden_size: u64,
    pub last_modified: i64,
    pub is_hidden: bool,
    pub permissions: String,
    /// Top extensions in this subtree by size (largest first, ≤5 entries).
    pub file_types: Vec<FileTypeStat>,
    /// Summed size of every extension beyond the top 5 — the "Other" slice.
    pub file_types_other: u64,
    /// Bucketed median mtime of file descendants (unix seconds). 0 = no files.
    pub median_mtime: i64,
}

/// One slice of a node's file-type composition.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTypeStat {
    /// Lowercased extension without the dot; empty for extensionless files.
    pub ext: String,
    pub size: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub root_id: String,
    pub total_size: u64,
    pub total_files: u64,
    pub total_directories: u64,
    pub scan_duration_ms: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgressDto {
    pub current_path: String,
    pub files_scanned: u64,
    pub directories_scanned: u64,
    pub bytes_scanned: u64,
    pub percent: f64,
    pub is_completed: bool,
}

impl ScanProgressDto {
    fn running(p: &Progress) -> Self {
        Self {
            current_path: p.current_path.clone(),
            files_scanned: p.files_scanned,
            directories_scanned: p.directories_scanned,
            bytes_scanned: p.bytes_scanned,
            percent: p.percent,
            is_completed: false,
        }
    }
}

fn display_name(tree: &ScanTree, idx: u32) -> String {
    let node = &tree.nodes[idx as usize];
    if node.parent.is_none() {
        tree.root_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| tree.root_path.to_string_lossy().to_string())
    } else {
        node.name.clone()
    }
}

/// Never roll a folder this small into the "Other" bucket — show all of it.
const MIN_SHOWN: usize = 12;

/// How many of a node's (size-sorted) children to show before the rest collapse
/// into "Other". Returns the smallest N in `[MIN_SHOWN, max]` over the suffix
/// `children[offset..]` such that the hidden remainder is no larger than the
/// smallest shown child — so "Other" can never be the biggest tile. Falls back
/// to the ceiling when the tail never decays enough (e.g. thousands of equal
/// files), where a large "Other" is genuine and stays drillable via `offset`.
fn adaptive_visible_count(tree: &ScanTree, children: &[u32], offset: usize, max: usize) -> usize {
    let rem = &children[offset.min(children.len())..];
    let l = rem.len();
    if l <= MIN_SHOWN {
        return l;
    }
    let sizes: Vec<u64> = rem.iter().map(|&c| tree.nodes[c as usize].size).collect();
    let total: u64 = sizes.iter().sum();
    let upper = l.min(max);
    let mut shown_sum: u64 = sizes[..MIN_SHOWN].iter().sum();
    let mut n = MIN_SHOWN;
    loop {
        let hidden = total - shown_sum;
        if hidden <= sizes[n - 1] || n >= upper {
            return n;
        }
        shown_sum += sizes[n];
        n += 1;
    }
}

/// Lowercased extension (no dot) for a file name; empty if it has none.
fn extension_of(name: &str) -> String {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Upper bounds (exclusive, in days) of each age bucket; the last bucket is
/// open-ended. Coarse on purpose — the median is a rough "is this stale?" signal.
const AGE_BUCKET_DAYS: [i64; 8] = [1, 7, 30, 90, 180, 365, 730, 1825];
/// Representative age (days) reported for a file landing in each bucket.
const AGE_BUCKET_REP_DAYS: [i64; 9] = [0, 3, 18, 60, 135, 270, 545, 1277, 2555];
const SECS_PER_DAY: i64 = 86_400;

fn age_bucket(age_days: i64) -> usize {
    AGE_BUCKET_DAYS
        .iter()
        .position(|&b| age_days < b)
        .unwrap_or(AGE_BUCKET_DAYS.len())
}

/// Walk the subtree rooted at `idx` and summarise its file descendants:
/// top-5 extensions by size (+ the summed remainder), and a bucketed-median
/// mtime. Directory mtimes are ignored. Iterative to avoid deep recursion.
fn subtree_stats(tree: &ScanTree, idx: u32) -> (Vec<FileTypeStat>, u64, i64) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let mut ext_sizes: HashMap<String, u64> = HashMap::new();
    let mut age_hist = [0u64; 9];
    let mut file_count: u64 = 0;

    let mut stack: Vec<u32> = vec![idx];
    while let Some(cur) = stack.pop() {
        let node = &tree.nodes[cur as usize];
        if node.is_dir {
            stack.extend_from_slice(&node.children);
            continue;
        }
        // File: tally its extension bytes and age bucket.
        *ext_sizes.entry(extension_of(&node.name)).or_insert(0) += node.size;
        let age_days = ((now - node.mtime).max(0)) / SECS_PER_DAY;
        age_hist[age_bucket(age_days)] += 1;
        file_count += 1;
    }

    let mut sorted: Vec<FileTypeStat> = ext_sizes
        .into_iter()
        .map(|(ext, size)| FileTypeStat { ext, size })
        .collect();
    sorted.sort_unstable_by(|a, b| b.size.cmp(&a.size).then_with(|| a.ext.cmp(&b.ext)));

    let other: u64 = sorted.iter().skip(5).map(|s| s.size).sum();
    sorted.truncate(5);

    let median_mtime = if file_count == 0 {
        0
    } else {
        let target = (file_count + 1) / 2;
        let mut cumulative = 0u64;
        let mut bucket = 0usize;
        for (i, &count) in age_hist.iter().enumerate() {
            cumulative += count;
            if cumulative >= target {
                bucket = i;
                break;
            }
        }
        now - AGE_BUCKET_REP_DAYS[bucket] * SECS_PER_DAY
    };

    (sorted, other, median_mtime)
}

fn build_dto(
    tree: &ScanTree,
    idx: u32,
    depth_left: usize,
    max_children: usize,
    offset: usize,
) -> FileNodeDto {
    let node = &tree.nodes[idx as usize];
    let mut children = Vec::new();
    if node.is_dir && depth_left > 0 {
        // `offset` paginates this node's (size-sorted) children so the UI can
        // drill into the "Other" bucket. Nested levels always start at 0.
        // `max_children` is the hard ceiling; the shown count adapts below it.
        let visible = adaptive_visible_count(tree, &node.children, offset, max_children);
        for &c in node.children.iter().skip(offset).take(visible) {
            children.push(build_dto(tree, c, depth_left - 1, max_children, 0));
        }
    }
    // Aggregate the immediate children that were truncated away so the UI can
    // render an "Other" bucket. Always reflects the full child list regardless
    // of `depth_left`, so a leaf-depth directory still reports what it hides.
    let consumed = offset + children.len();
    let hidden_children = node.children.len().saturating_sub(consumed) as u64;
    let hidden_size: u64 = node
        .children
        .iter()
        .skip(consumed)
        .map(|&c| tree.nodes[c as usize].size)
        .sum();
    let (file_types, file_types_other, median_mtime) = subtree_stats(tree, idx);
    FileNodeDto {
        id: idx.to_string(),
        name: display_name(tree, idx),
        path: tree.path_of(idx).to_string_lossy().to_string(),
        node_type: if node.is_dir { "directory" } else { "file" }.to_string(),
        size: node.size,
        file_count: node.file_count,
        dir_count: node.dir_count,
        children,
        hidden_children,
        hidden_size,
        last_modified: node.mtime,
        is_hidden: node.is_hidden,
        permissions: String::new(),
        file_types,
        file_types_other,
        median_mtime,
    }
}

/// Scan a directory. Runs the (CPU/IO heavy) walk on a blocking thread,
/// streams `scan-progress` events, stores the tree, and returns totals.
#[tauri::command]
pub async fn scan_directory(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<ScanSummary, String> {
    let root = PathBuf::from(&path);
    let app_for_progress = app.clone();

    // Fresh run: clear any leftover cancellation request from a prior scan.
    let cancel = state.cancel.clone();
    cancel.store(false, Ordering::Relaxed);

    let scan_result = tauri::async_runtime::spawn_blocking(move || {
        scanner::scan(root, cancel, move |p| {
            let _ = app_for_progress.emit("scan-progress", ScanProgressDto::running(&p));
        })
    })
    .await
    .map_err(|e| e.to_string())?;

    let tree = match scan_result {
        Ok(tree) => tree,
        // Distinct, non-error sentinel so the frontend can silently restore the
        // prior view instead of surfacing a scan-failure card.
        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
            return Err("cancelled".to_string())
        }
        Err(e) => return Err(e.to_string()),
    };

    let summary = ScanSummary {
        root_id: tree.root.to_string(),
        total_size: tree.total_size,
        total_files: tree.total_files,
        total_directories: tree.total_dirs,
        scan_duration_ms: tree.scan_duration_ms,
    };

    let _ = app.emit(
        "scan-progress",
        ScanProgressDto {
            current_path: String::new(),
            files_scanned: tree.total_files,
            directories_scanned: tree.total_dirs,
            bytes_scanned: tree.total_size,
            percent: 100.0,
            is_completed: true,
        },
    );

    *state.tree.lock().unwrap() = Some(tree);
    Ok(summary)
}

/// Return a bounded slice of the scanned tree rooted at `node_id`.
#[tauri::command]
pub fn get_subtree(
    state: State<'_, AppState>,
    node_id: String,
    max_depth: Option<usize>,
    max_children: Option<usize>,
    offset: Option<usize>,
) -> Result<FileNodeDto, String> {
    let idx: u32 = node_id.parse().map_err(|_| "invalid node id".to_string())?;
    let guard = state.tree.lock().unwrap();
    let tree = guard.as_ref().ok_or("no scan loaded")?;
    if idx as usize >= tree.nodes.len() {
        return Err("node id out of range".into());
    }
    Ok(build_dto(
        tree,
        idx,
        max_depth.unwrap_or(3),
        max_children.unwrap_or(100),
        offset.unwrap_or(0),
    ))
}

#[tauri::command]
pub fn get_home_directory() -> String {
    dirs_home().to_string_lossy().to_string()
}

#[tauri::command]
pub fn get_common_directories() -> Vec<String> {
    let home = dirs_home();
    ["Desktop", "Documents", "Downloads", "Pictures", "Movies", "Music"]
        .iter()
        .map(|d| home.join(d))
        .filter(|p| p.is_dir())
        .map(|p| p.to_string_lossy().to_string())
        .collect()
}

#[tauri::command]
pub fn validate_path(path: String) -> bool {
    let p = PathBuf::from(path);
    p.is_dir()
}

/// Move a path to the system Trash (reversible — safer than hard delete).
#[tauri::command]
pub fn delete_path(path: String) -> Result<(), String> {
    trash::delete(&path).map_err(|e| e.to_string())
}

/// Move a scanned node to the system Trash and update the in-memory tree
/// incrementally — subtracts its size from all ancestors, detaches it from
/// its parent, and tombstones it. Returns updated totals so the frontend
/// can refresh its summary cards without triggering a full rescan.
#[tauri::command]
pub fn delete_node(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<ScanSummary, String> {
    let idx: u32 = node_id.parse().map_err(|_| "invalid node id".to_string())?;
    let mut guard = state.tree.lock().unwrap();
    let tree = guard.as_mut().ok_or("no scan loaded")?;
    if idx as usize >= tree.nodes.len() {
        return Err("node id out of range".into());
    }

    // Reconstruct the filesystem path before mutating the arena.
    let path = tree.path_of(idx);

    // Move to Trash (reversible).
    trash::delete(&path).map_err(|e| e.to_string())?;

    // Subtract sizes from ancestors, detach node, and tombstone it in place.
    let (total_size, total_files, total_dirs) = tree.remove_subtree(idx);

    Ok(ScanSummary {
        root_id: tree.root.to_string(),
        total_size,
        total_files,
        total_directories: total_dirs,
        scan_duration_ms: tree.scan_duration_ms,
    })
}

/// Request cancellation of the in-flight scan. The running `scanner::scan`
/// polls this flag and aborts with `ErrorKind::Interrupted`, which
/// `scan_directory` reports back as the "cancelled" sentinel.
#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn open_in_finder(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| e.to_string())
}

fn dirs_home() -> PathBuf {
    #[cfg(unix)]
    {
        if let Ok(h) = std::env::var("HOME") {
            return PathBuf::from(h);
        }
    }
    #[cfg(windows)]
    {
        if let Ok(h) = std::env::var("USERPROFILE") {
            return PathBuf::from(h);
        }
    }
    PathBuf::from("/")
}
