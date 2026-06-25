//! Tauri command surface. The scanned tree lives here in managed state; the
//! frontend pulls only the bounded slice it renders via `get_subtree`, so IPC
//! payloads stay tiny regardless of how many millions of files were scanned.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::scanner::{self, Progress, ScanTree};
use crate::scanner::{AGE_BUCKET_REP_DAYS, SECS_PER_DAY};

#[derive(Default)]
pub struct AppState {
    pub tree: Mutex<Option<ScanTree>>,
    pub cancel: Arc<AtomicBool>,
}

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
    pub hidden_children: u64,
    pub hidden_size: u64,
    pub last_modified: i64,
    pub is_hidden: bool,
    pub permissions: String,
    pub file_types: Vec<FileTypeStat>,
    pub file_types_other: u64,
    pub median_mtime: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTypeStat {
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

const MIN_SHOWN: usize = 12;

fn adaptive_visible_count(tree: &ScanTree, children: &[u32], offset: usize, max: usize) -> usize {
    let rem = &children[offset.min(children.len())..];
    let l = rem.len();
    if l <= MIN_SHOWN { return l; }
    let sizes: Vec<u64> = rem.iter().map(|&c| tree.nodes[c as usize].size).collect();
    let total: u64 = sizes.iter().sum();
    let upper = l.min(max);
    let mut shown_sum: u64 = sizes[..MIN_SHOWN].iter().sum();
    let mut n = MIN_SHOWN;
    loop {
        let hidden = total - shown_sum;
        if hidden <= sizes[n - 1] || n >= upper { return n; }
        shown_sum += sizes[n];
        n += 1;
    }
}

/// O(1) subtree stats lookup using precomputed `dir_stats`.
/// Returns (file_types[≤8], file_types_other, median_mtime).
fn subtree_stats(tree: &ScanTree, idx: u32) -> (Vec<FileTypeStat>, u64, i64) {
    let Some(stats) = tree.dir_stats.get(&idx) else {
        return (Vec::new(), 0, 0);
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let file_types: Vec<FileTypeStat> = stats.exts.iter().take(8).map(|&(ext_id, size)| {
        FileTypeStat {
            ext: tree.ext_names[ext_id as usize].clone(),
            size,
        }
    }).collect();
    let file_types_other: u64 = stats.exts.iter().skip(8).map(|&(_, sz)| sz).sum();

    let file_count: u64 = stats.age_hist.iter().map(|&x| x as u64).sum();
    let median_mtime = if file_count == 0 {
        0
    } else {
        let target = (file_count + 1) / 2;
        let mut cumulative = 0u64;
        let mut bucket = 0usize;
        for (i, &count) in stats.age_hist.iter().enumerate() {
            cumulative += count as u64;
            if cumulative >= target { bucket = i; break; }
        }
        now - AGE_BUCKET_REP_DAYS[bucket] * SECS_PER_DAY
    };

    (file_types, file_types_other, median_mtime)
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
        let visible = adaptive_visible_count(tree, &node.children, offset, max_children);
        for &c in node.children.iter().skip(offset).take(visible) {
            children.push(build_dto(tree, c, depth_left - 1, max_children, 0));
        }
    }
    let consumed = offset + children.len();
    let hidden_children = node.children.len().saturating_sub(consumed) as u64;
    let hidden_size: u64 = node.children.iter().skip(consumed)
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

#[tauri::command]
pub async fn scan_directory(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<ScanSummary, String> {
    let root = PathBuf::from(&path);
    let app_for_progress = app.clone();

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
    Ok(build_dto(tree, idx, max_depth.unwrap_or(3), max_children.unwrap_or(100), offset.unwrap_or(0)))
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
    PathBuf::from(path).is_dir()
}

#[tauri::command]
pub fn delete_path(path: String) -> Result<(), String> {
    trash::delete(&path).map_err(|e| e.to_string())
}

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
    let path = tree.path_of(idx);
    trash::delete(&path).map_err(|e| e.to_string())?;
    let (total_size, total_files, total_dirs) = tree.remove_subtree(idx);
    Ok(ScanSummary {
        root_id: tree.root.to_string(),
        total_size,
        total_files,
        total_directories: total_dirs,
        scan_duration_ms: tree.scan_duration_ms,
    })
}

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
    if let Ok(h) = std::env::var("HOME") { return PathBuf::from(h); }
    #[cfg(windows)]
    if let Ok(h) = std::env::var("USERPROFILE") { return PathBuf::from(h); }
    PathBuf::from("/")
}
