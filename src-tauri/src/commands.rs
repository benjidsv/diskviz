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
    // Last-resort fallback if the env var is unset — "/" is meaningless on
    // Windows, so pick a per-platform root that at least exists.
    #[cfg(windows)]
    return PathBuf::from("C:\\");
    #[cfg(not(windows))]
    PathBuf::from("/")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{AGE_BUCKET_REP_DAYS, DirStats, Node, ScanTree, SECS_PER_DAY};
    use std::collections::HashMap;
    use std::path::PathBuf;

    // ── Shared fixture builder ─────────────────────────────────────────────────

    /// Build a minimal `ScanTree` with a root directory containing `n` file
    /// children whose sizes are given by `sizes` (largest first). The helper
    /// assigns ascending ext-ids so each file has a distinct extension.
    fn build_test_tree(sizes: &[u64]) -> ScanTree {
        let mut nodes: Vec<Node> = Vec::new();

        // Root directory (index 0).
        let child_indices: Vec<u32> = (1..=sizes.len() as u32).collect();
        nodes.push(Node {
            name: "/root".into(),
            size: sizes.iter().sum(),
            file_count: sizes.len() as u64,
            dir_count: 0,
            is_dir: true,
            is_hidden: false,
            mtime: 0,
            parent: None,
            children: child_indices.clone(),
        });

        // One file child per size (already sorted largest-first per the test).
        let mut ext_names: Vec<String> = Vec::new();
        let mut age_hist = [0u32; 9];

        for (i, &sz) in sizes.iter().enumerate() {
            let ext = format!("ext{}", i);
            ext_names.push(ext);
            nodes.push(Node {
                name: format!("file{}.ext{}", i, i),
                size: sz,
                file_count: 0,
                dir_count: 0,
                is_dir: false,
                is_hidden: false,
                mtime: 0,
                parent: Some(0),
                children: vec![],
            });
            age_hist[0] += 1; // all files land in bucket 0 (age_days == 0)
        }

        // Build dir_stats for root: each ext has the corresponding size.
        let exts: Vec<(u32, u64)> = sizes
            .iter()
            .enumerate()
            .map(|(i, &sz)| (i as u32, sz))
            .collect();

        let mut dir_stats: HashMap<u32, DirStats> = HashMap::new();
        dir_stats.insert(
            0,
            DirStats {
                age_hist,
                exts,
            },
        );

        ScanTree {
            nodes,
            root: 0,
            root_path: PathBuf::from("/root"),
            total_size: sizes.iter().sum(),
            total_files: sizes.len() as u64,
            total_dirs: 0,
            scan_duration_ms: 0,
            ext_names,
            dir_stats,
            scan_now_secs: 0,
        }
    }

    // ── adaptive_visible_count ─────────────────────────────────────────────────

    #[test]
    fn avc_len_le_min_shown_returns_all() {
        // With ≤ MIN_SHOWN (12) children the function must return all of them.
        let sizes: Vec<u64> = (1..=12).map(|i| i * 1000).rev().collect(); // 12..1 kB
        let tree = build_test_tree(&sizes);
        let children = &tree.nodes[0].children;
        assert_eq!(
            adaptive_visible_count(&tree, children, 0, 200),
            12,
            "≤ MIN_SHOWN → all returned"
        );
    }

    #[test]
    fn avc_other_is_never_biggest_tile() {
        // The invariant: hidden ≤ sizes[n-1] (the smallest shown tile).
        // Use 20 files with exponentially dropping sizes so that the
        // initial MIN_SHOWN window would leave a large hidden remainder.
        let sizes: Vec<u64> = (0..20u64).map(|i| 1_000_000 / (i + 1)).collect();
        let tree = build_test_tree(&sizes);
        let children = &tree.nodes[0].children;
        let n = adaptive_visible_count(&tree, children, 0, 200);
        let node_sizes: Vec<u64> = children.iter().map(|&c| tree.nodes[c as usize].size).collect();
        let total: u64 = node_sizes.iter().sum();
        let shown_sum: u64 = node_sizes[..n].iter().sum();
        let hidden = total - shown_sum;
        assert!(
            hidden <= node_sizes[n - 1],
            "hidden ({hidden}) must be ≤ smallest shown tile ({})",
            node_sizes[n - 1]
        );
    }

    #[test]
    fn avc_respects_max_clamp() {
        // When max is less than what the invariant would choose, the result
        // is clamped to max.
        let sizes: Vec<u64> = (1..=30u64).map(|i| 1_000_000 / i).collect();
        let tree = build_test_tree(&sizes);
        let children = &tree.nodes[0].children;
        let max = 15usize;
        let n = adaptive_visible_count(&tree, children, 0, max);
        assert!(n <= max, "n ({n}) must be ≤ max ({max})");
    }

    #[test]
    fn avc_offset_past_end_returns_zero() {
        let sizes = vec![100u64, 200, 300];
        let tree = build_test_tree(&sizes);
        let children = &tree.nodes[0].children;
        // Offset beyond child count → rem is empty → len=0 ≤ MIN_SHOWN → returns 0.
        assert_eq!(adaptive_visible_count(&tree, children, 100, 200), 0);
    }

    // ── subtree_stats ──────────────────────────────────────────────────────────

    #[test]
    fn subtree_stats_empty_returns_zero_triple() {
        let tree = build_test_tree(&[]);
        // Root has a DirStats entry but age_hist is all zeros (no files).
        let (ft, other, median) = subtree_stats(&tree, 0);
        assert!(ft.is_empty(), "no exts");
        assert_eq!(other, 0);
        assert_eq!(median, 0, "no files → median 0");
    }

    #[test]
    fn subtree_stats_no_dir_stats_returns_zero_triple() {
        let tree = build_test_tree(&[100, 200]);
        // Query a node index that has no DirStats (a file node, e.g. idx 1).
        let (ft, other, _median) = subtree_stats(&tree, 1);
        assert!(ft.is_empty());
        assert_eq!(other, 0);
    }

    #[test]
    fn subtree_stats_top_8_exts_and_other() {
        // Build a tree with 10 extensions; the last 2 should be folded into other.
        // Sizes are arranged largest-first.
        let sizes: Vec<u64> = (1..=10u64).map(|i| (11 - i) * 1000).collect();
        let tree = build_test_tree(&sizes);
        let (ft, other, _median) = subtree_stats(&tree, 0);
        assert_eq!(ft.len(), 8, "top-8 ext types returned");
        // The two smallest extensions are summed into other.
        let expected_other: u64 = sizes[8] + sizes[9];
        assert_eq!(other, expected_other, "file_types_other is sum of exts beyond top-8");
    }

    #[test]
    fn subtree_stats_exactly_8_exts_no_other() {
        let sizes: Vec<u64> = (1..=8u64).map(|i| (9 - i) * 1000).collect();
        let tree = build_test_tree(&sizes);
        let (ft, other, _median) = subtree_stats(&tree, 0);
        assert_eq!(ft.len(), 8);
        assert_eq!(other, 0, "exactly 8 exts → no remainder");
    }

    #[test]
    fn subtree_stats_median_bucket_index() {
        // Build a tree where all files land in age_hist bucket 2 (7..30 days).
        // We manually insert a DirStats entry with the desired histogram.
        let mut tree = build_test_tree(&[100, 200, 300]);

        // Override dir_stats: put 3 files into bucket 2.
        tree.dir_stats.insert(
            0,
            DirStats {
                age_hist: [0, 0, 3, 0, 0, 0, 0, 0, 0],
                exts: vec![],
            },
        );

        let (_ft, _other, median_mtime) = subtree_stats(&tree, 0);

        // Median falls in bucket 2; representative days = AGE_BUCKET_REP_DAYS[2].
        // median_mtime = now - REP_DAYS[2] * SECS_PER_DAY
        // We don't freeze the clock, so check the structure: the result must be
        // a Unix timestamp approximately (now - REP_DAYS[2] * SECS_PER_DAY).
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let rep_days = AGE_BUCKET_REP_DAYS[2];
        let expected = now - rep_days * SECS_PER_DAY;
        // Allow ±5 s tolerance for test execution lag.
        assert!(
            (median_mtime - expected).abs() < 5,
            "median_mtime ({median_mtime}) should be close to now - REP_DAYS[2]*SECS_PER_DAY ({expected})"
        );
    }
}

