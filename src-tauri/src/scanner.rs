//! Fast parallel filesystem scanner.
//!
//! Walks a directory tree in parallel (via `jwalk`, the engine behind `dust`)
//! and stores the result in a compact index-based arena (`Vec<Node>`) instead
//! of a pointer tree. Directory sizes are aggregated bottom-up in a single
//! reverse pass. The full tree stays in Rust memory; the frontend only ever
//! pulls the bounded slice it is currently rendering (see `commands::get_subtree`).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Instant, UNIX_EPOCH};

use jwalk::WalkDirGeneric;

/// Per-entry metadata computed in parallel inside `process_read_dir`.
#[derive(Clone, Debug, Default)]
pub struct EntryMeta {
    size: u64,
    mtime: i64,
}

/// A single node in the arena. Indices, not pointers — cache friendly and
/// compact. The full path is never stored; it is reconstructed on demand by
/// walking `parent` (see [`ScanTree::path_of`]).
pub struct Node {
    pub name: String,
    pub size: u64,
    /// Number of file descendants in this node's subtree.
    pub file_count: u64,
    /// Number of directory descendants (excluding this node).
    pub dir_count: u64,
    pub is_dir: bool,
    pub is_hidden: bool,
    pub mtime: i64,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
}

/// The scanned tree plus aggregate totals.
pub struct ScanTree {
    pub nodes: Vec<Node>,
    pub root: u32,
    pub root_path: PathBuf,
    pub total_size: u64,
    pub total_files: u64,
    pub total_dirs: u64,
    pub scan_duration_ms: u64,
}

impl ScanTree {
    /// Reconstruct the absolute path of a node by walking up the parent chain.
    pub fn path_of(&self, mut idx: u32) -> PathBuf {
        let mut parts: Vec<&str> = Vec::new();
        loop {
            let node = &self.nodes[idx as usize];
            match node.parent {
                Some(p) => {
                    parts.push(&node.name);
                    idx = p;
                }
                None => break, // root: name holds the full root path
            }
        }
        let mut path = self.root_path.clone();
        for part in parts.iter().rev() {
            path.push(part);
        }
        path
    }

    /// Remove a node (and its whole subtree) from the arena: subtracts its
    /// aggregated size/counts from all ancestors and detaches it from its
    /// parent's children list. Tombstones the node in place so arena indices
    /// remain stable (the frontend holds indices as opaque node ids).
    ///
    /// Mirrors the bottom-up aggregation from `scan()` in reverse for a single
    /// deletion path. Returns the updated tree-level totals.
    pub fn remove_subtree(&mut self, idx: u32) -> (u64, u64, u64) {
        let node = &self.nodes[idx as usize];
        let d_size  = node.size;
        // Mirror the scan-time aggregation: a file contributes +1 to its
        // parent's file_count; a directory contributes +1 to dir_count.
        let d_files = node.file_count + if node.is_dir { 0 } else { 1 };
        let d_dirs  = node.dir_count  + if node.is_dir { 1 } else { 0 };
        let parent_opt = node.parent;

        // Walk up the parent chain and subtract deltas from every ancestor.
        // NLL lets us reborrow nodes[cur] on each iteration.
        let mut cur_opt = parent_opt;
        while let Some(cur) = cur_opt {
            let anc = &mut self.nodes[cur as usize];
            anc.size       = anc.size.saturating_sub(d_size);
            anc.file_count = anc.file_count.saturating_sub(d_files);
            anc.dir_count  = anc.dir_count.saturating_sub(d_dirs);
            cur_opt = anc.parent; // copy Option<u32> out before borrow ends
        }

        // Detach from parent's children list.
        if let Some(p) = parent_opt {
            self.nodes[p as usize].children.retain(|&c| c != idx);
        }

        // Tombstone: zero out the node so stale get_subtree calls are harmless.
        let node = &mut self.nodes[idx as usize];
        node.size       = 0;
        node.file_count = 0;
        node.dir_count  = 0;
        node.children.clear();

        // Keep tree-level totals consistent with the per-node data.
        self.total_size  = self.total_size.saturating_sub(d_size);
        self.total_files = self.total_files.saturating_sub(d_files);
        self.total_dirs  = self.total_dirs.saturating_sub(d_dirs);

        (self.total_size, self.total_files, self.total_dirs)
    }
}

/// Live progress emitted while scanning.
#[derive(Clone, Debug)]
pub struct Progress {
    pub current_path: String,
    pub files_scanned: u64,
    pub directories_scanned: u64,
    pub bytes_scanned: u64,
    pub percent: f64,
}

/// Scan `root` in parallel. `on_progress` is invoked periodically (throttled by
/// the caller's appetite — we call it at most every ~80ms / 4k entries).
///
/// `cancel` is polled both on the worker threads (to stop descending) and on the
/// consuming thread; when set, the scan aborts early with `ErrorKind::Interrupted`.
pub fn scan<F: FnMut(Progress)>(
    root: PathBuf,
    cancel: Arc<AtomicBool>,
    mut on_progress: F,
) -> std::io::Result<ScanTree> {
    let start = Instant::now();

    // Denominator for a genuine progress bar: bytes already used on the volume
    // that holds `root`. For whole-disk / home scans this is a close, monotonic
    // proxy, so the bar fills smoothly toward 100%.
    let denom = volume_used_bytes(&root).unwrap_or(0);

    // The walk is latency-bound on one lstat() per entry, so oversubscribe the
    // thread pool (~2x cores) to hide IO stalls behind other directories' stats.
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8)
        .saturating_mul(2)
        .max(4);

    // Inode dedup set, shared across the rayon workers. Each (device, inode) is
    // counted once — exactly like `du`. This is essential on macOS, where the
    // same data is reachable through firmlinks (`/Users`) AND direct mounts
    // (`/System/Volumes/Data/Users`); without it a whole-disk scan counts the
    // Data volume several times. Also dedups hardlinks (pnpm stores, backups).
    // Only directories and multiply-linked files are tracked, keeping it small.
    #[cfg(unix)]
    let visited: dashmap::DashSet<(u64, u64)> = dashmap::DashSet::new();

    let cancel_walk = cancel.clone();
    let walk = WalkDirGeneric::<((), EntryMeta)>::new(&root)
        .skip_hidden(false)
        .follow_links(false)
        .parallelism(jwalk::Parallelism::RayonNewPool(threads))
        .process_read_dir(move |_depth, _path, _read_dir_state, children| {
            // Cancelled mid-scan: drop this directory's children so jwalk stops
            // descending. The consumer loop below also bails on the same flag.
            if cancel_walk.load(Ordering::Relaxed) {
                children.clear();
                return;
            }
            // Runs on the rayon worker handling this directory, so the stat()
            // calls parallelize across directories — the core speed win.
            for entry in children.iter_mut().flatten() {
                if let Ok(meta) = entry.metadata() {
                    let mtime = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    let is_dir = meta.is_dir();

                    // Allocated size (actual blocks on disk), not apparent size.
                    // This matches `du` / WizTree / DaisyDisk and correctly
                    // handles sparse files, APFS clones and transparent
                    // compression — apparent size wildly overcounts VM/Docker
                    // disk images and the like.
                    #[cfg(unix)]
                    let size = {
                        use std::os::unix::fs::MetadataExt;
                        meta.blocks().saturating_mul(512)
                    };
                    #[cfg(not(unix))]
                    let size = meta.len();

                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::MetadataExt;
                        if is_dir || meta.nlink() > 1 {
                            // `insert` returns false if the key already existed.
                            if !visited.insert((meta.dev(), meta.ino())) {
                                // Already counted via another path: zero it out
                                // and stop jwalk from descending again.
                                entry.client_state = EntryMeta { size: 0, mtime };
                                if is_dir {
                                    entry.read_children_path = None;
                                }
                                continue;
                            }
                        }
                    }

                    entry.client_state = EntryMeta { size, mtime };
                }
            }
        });

    let mut nodes: Vec<Node> = Vec::new();
    // Map a directory's emitted index by its depth so each entry can find its
    // parent. jwalk streams parents before their children, so the entry at the
    // immediately-shallower depth seen most recently is the parent.
    let mut parent_at_depth: Vec<u32> = Vec::new();

    let mut total_files: u64 = 0;
    let mut total_dirs: u64 = 0;
    let mut bytes_scanned: u64 = 0;
    let mut last_emit = Instant::now();
    let mut since_emit: u64 = 0;

    let mut root_idx: Option<u32> = None;

    for entry in walk {
        if cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "scan cancelled",
            ));
        }
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let depth = entry.depth();
        let is_dir = entry.file_type().is_dir();
        let meta = entry.client_state.clone();

        let name = if depth == 0 {
            entry.path().to_string_lossy().to_string()
        } else {
            entry.file_name().to_string_lossy().to_string()
        };
        let is_hidden = depth != 0 && name.starts_with('.');

        let idx = nodes.len() as u32;
        let parent = if depth == 0 {
            None
        } else {
            parent_at_depth.get(depth - 1).copied()
        };

        nodes.push(Node {
            name,
            size: meta.size,
            file_count: 0,
            dir_count: 0,
            is_dir,
            is_hidden,
            mtime: meta.mtime,
            parent,
            children: Vec::new(),
        });

        if let Some(p) = parent {
            nodes[p as usize].children.push(idx);
        }

        // Record this entry as the potential parent for the next depth down.
        if parent_at_depth.len() <= depth {
            parent_at_depth.resize(depth + 1, 0);
        }
        parent_at_depth[depth] = idx;

        if depth == 0 {
            root_idx = Some(idx);
        }

        if is_dir {
            total_dirs += 1;
        } else {
            total_files += 1;
            bytes_scanned += meta.size;
        }

        since_emit += 1;
        if since_emit >= 4096 || last_emit.elapsed().as_millis() >= 80 {
            since_emit = 0;
            last_emit = Instant::now();
            let percent = if denom > 0 {
                ((bytes_scanned as f64 / denom as f64) * 100.0).min(99.0)
            } else {
                // No denominator: nudge toward 95% asymptotically so the bar
                // still moves but never claims completion early.
                95.0 * (1.0 - 1.0 / (1.0 + (total_files + total_dirs) as f64 / 50_000.0))
            };
            on_progress(Progress {
                current_path: entry.path().to_string_lossy().to_string(),
                files_scanned: total_files,
                directories_scanned: total_dirs,
                bytes_scanned,
                percent,
            });
        }
    }

    let root_index = root_idx.ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "root could not be scanned")
    })?;

    // Bottom-up size aggregation. Children always have a higher index than
    // their parent (parents are pushed first), so a single reverse pass sums
    // each subtree before its parent is folded into the grandparent.
    for i in (0..nodes.len()).rev() {
        if let Some(p) = nodes[i].parent {
            let s = nodes[i].size;
            let (fc, dc, is_dir) = (nodes[i].file_count, nodes[i].dir_count, nodes[i].is_dir);
            let parent = &mut nodes[p as usize];
            parent.size += s;
            parent.file_count += fc + if is_dir { 0 } else { 1 };
            parent.dir_count += dc + if is_dir { 1 } else { 0 };
        }
    }

    // Sort each node's children largest-first, once, so the UI never has to.
    for i in 0..nodes.len() {
        let mut kids = std::mem::take(&mut nodes[i].children);
        kids.sort_unstable_by(|&a, &b| nodes[b as usize].size.cmp(&nodes[a as usize].size));
        nodes[i].children = kids;
    }

    // Final totals come from the aggregated root so they match the per-node
    // `file_count` / `dir_count` the UI shows (descendants, root excluded).
    let total_size = nodes[root_index as usize].size;
    let total_files = nodes[root_index as usize].file_count;
    let total_dirs = nodes[root_index as usize].dir_count;

    Ok(ScanTree {
        nodes,
        root: root_index,
        root_path: root,
        total_size,
        total_files,
        total_dirs,
        scan_duration_ms: start.elapsed().as_millis() as u64,
    })
}

/// Bytes currently used on the volume containing `path` (via statvfs).
#[cfg(unix)]
fn volume_used_bytes(path: &std::path::Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut stat) != 0 {
            return None;
        }
        let frsize = stat.f_frsize as u64;
        let used_blocks = stat.f_blocks.saturating_sub(stat.f_bfree) as u64;
        Some(used_blocks.saturating_mul(frsize))
    }
}

#[cfg(not(unix))]
fn volume_used_bytes(_path: &std::path::Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn aggregates_sizes_and_counts() {
        // Build a known tree:
        //   root/
        //     a.bin            (100 bytes)
        //     sub/
        //       b.bin          (200 bytes)
        //       c.bin          (300 bytes)
        //       deep/
        //         d.bin        (400 bytes)
        let root = std::env::temp_dir().join(format!("diskviz_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sub/deep")).unwrap();
        fs::write(root.join("a.bin"), vec![0u8; 100]).unwrap();
        fs::write(root.join("sub/b.bin"), vec![0u8; 200]).unwrap();
        fs::write(root.join("sub/c.bin"), vec![0u8; 300]).unwrap();
        fs::write(root.join("sub/deep/d.bin"), vec![0u8; 400]).unwrap();

        let cancel = Arc::new(AtomicBool::new(false));
        let tree = scan(root.clone(), cancel, |_| {}).unwrap();

        // Counts are exact; byte totals use allocated blocks, so we assert
        // consistency and ordering rather than an exact byte count (block size
        // is filesystem dependent).
        assert_eq!(tree.total_files, 4, "four files");
        assert_eq!(tree.total_dirs, 2, "sub and sub/deep (root excluded)");
        assert!(tree.total_size >= 1000, "allocated size >= logical bytes");

        let root_node = &tree.nodes[tree.root as usize];
        assert_eq!(root_node.size, tree.total_size, "root holds the whole total");
        assert_eq!(root_node.file_count, 4);
        assert_eq!(root_node.dir_count, 2);

        // Largest child of root is `sub` (holds 3 of the 4 files) — sorted first.
        let sub = &tree.nodes[root_node.children[0] as usize];
        assert_eq!(sub.name, "sub");
        assert!(sub.size >= 900);
        assert_eq!(sub.file_count, 3);
        assert_eq!(sub.dir_count, 1);

        let _ = fs::remove_dir_all(&root);
    }
}
