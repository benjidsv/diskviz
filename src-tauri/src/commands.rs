//! Tauri command surface. The scanned tree lives here in managed state; the
//! frontend pulls only the bounded slice it renders via `get_subtree`, so IPC
//! payloads stay tiny regardless of how many millions of files were scanned.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::scanner::{self, Progress, ScanTree};

#[derive(Default)]
pub struct AppState {
    pub tree: Mutex<Option<ScanTree>>,
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
    pub last_modified: i64,
    pub is_hidden: bool,
    pub permissions: String,
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

fn build_dto(tree: &ScanTree, idx: u32, depth_left: usize, max_children: usize) -> FileNodeDto {
    let node = &tree.nodes[idx as usize];
    let mut children = Vec::new();
    if node.is_dir && depth_left > 0 {
        for &c in node.children.iter().take(max_children) {
            children.push(build_dto(tree, c, depth_left - 1, max_children));
        }
    }
    FileNodeDto {
        id: idx.to_string(),
        name: display_name(tree, idx),
        path: tree.path_of(idx).to_string_lossy().to_string(),
        node_type: if node.is_dir { "directory" } else { "file" }.to_string(),
        size: node.size,
        file_count: node.file_count,
        dir_count: node.dir_count,
        children,
        last_modified: node.mtime,
        is_hidden: node.is_hidden,
        permissions: String::new(),
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

    let tree = tauri::async_runtime::spawn_blocking(move || {
        scanner::scan(root, move |p| {
            let _ = app_for_progress.emit("scan-progress", ScanProgressDto::running(&p));
        })
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

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
        max_children.unwrap_or(20),
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
