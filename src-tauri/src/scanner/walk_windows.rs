//! Windows parallel directory walker using `GetFileInformationByHandleEx`
//! (`FileIdBothDirectoryInfo`) as the primary enumeration primitive — the
//! direct Win32 analog of macOS's `getattrlistbulk`. Each directory is
//! enumerated exactly once, returning name, allocated size, mtime, attributes,
//! and file-id in a single batch buffer call rather than a readdir + per-entry
//! `metadata()` stat.
//!
//! `scan()` in `mod.rs` dispatches here on Windows unless `DISKVIZ_NO_BULK=1`.
//!
//! Shared scaffolding (`RawNode`, `WalkStats`, `Msg`, `emit_progress_if_due`,
//! `flatten`) lives in `walk_common`.

use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use dashmap::DashSet;
use rayon::prelude::*;

use super::dirmeta;
use super::walk_common::{emit_progress_if_due, flatten, Msg, RawNode, WalkStats};

// ── Recursive parallel walk ───────────────────────────────────────────────────

/// Walk one directory and return its `RawNode` (with recursively built children).
///
/// Parallelism: subdirectories are processed via `rayon::par_iter`; the caller
/// must run this inside a `rayon::ThreadPool::install` scope.
///
/// `visited` is shared across threads; it prevents re-entering directory loops
/// (junctions can create cycles on Windows).
///
/// `tx` is a cloneable `Sender`; each recursive call receives an owned clone so
/// that all rayon workers can emit progress without a shared lock.
fn walk_dir(
    path:      PathBuf,
    name:      String,
    mtime:     i64,
    is_hidden: bool,
    visited:   &Arc<DashSet<(u64, u64)>>,
    cancel:    &Arc<AtomicBool>,
    stats:     &Arc<WalkStats>,
    denom:     u64,
    tx:        std::sync::mpsc::Sender<Msg>,
) -> RawNode {
    if cancel.load(Ordering::Relaxed) {
        return RawNode { name, size: 0, mtime, is_dir: true, is_hidden, children: vec![] };
    }

    let Some(entries) = dirmeta::bulk_dir_meta(&path) else {
        // Directory unreadable (permissions, etc.) — emit as an empty leaf.
        stats.dir_count.fetch_add(1, Ordering::Relaxed);
        return RawNode { name, size: 0, mtime, is_dir: true, is_hidden, children: vec![] };
    };

    stats.dir_count.fetch_add(1, Ordering::Relaxed);

    let mut file_nodes:   Vec<RawNode>                        = Vec::new();
    let mut subdir_tasks: Vec<(String, PathBuf, i64, u64, u64)> = Vec::new();

    for (fname, rm) in entries.into_iter() {
        let fname_str    = fname.to_string_lossy().into_owned();
        // Mirror the macOS convention: names starting with '.' are hidden.
        let is_child_hid = fname_str.starts_with('.');

        if rm.is_dir {
            // Never recurse into reparse points / junctions — matches
            // `follow_links(false)` behaviour of the jwalk fallback path.
            if rm.is_reparse {
                file_nodes.push(RawNode {
                    name: fname_str, size: 0, mtime: rm.mtime,
                    is_dir: true, is_hidden: is_child_hid, children: vec![],
                });
            } else if !visited.insert((rm.dev, rm.ino)) {
                // Already visited via a junction loop — emit as empty leaf.
                file_nodes.push(RawNode {
                    name: fname_str, size: 0, mtime: rm.mtime,
                    is_dir: true, is_hidden: is_child_hid, children: vec![],
                });
            } else {
                let subpath = path.join(&fname);
                subdir_tasks.push((fname_str, subpath, rm.mtime, rm.dev, rm.ino));
            }
        } else {
            // Windows hardlinks: nlink is always 1 from FileIdBothDirectoryInfo,
            // so we can't deduplicate on link count like the macOS path does.
            // This is acceptable — hardlinks are uncommon on NTFS consumer drives.
            stats.file_count.fetch_add(1, Ordering::Relaxed);
            stats.bytes_scanned.fetch_add(rm.size, Ordering::Relaxed);
            file_nodes.push(RawNode {
                name: fname_str, size: rm.size, mtime: rm.mtime,
                is_dir: false, is_hidden: is_child_hid, children: vec![],
            });
        }
    }

    emit_progress_if_due(&path.to_string_lossy(), stats, denom, &tx);

    // Recurse into subdirectories in parallel.
    let subdir_nodes: Vec<RawNode> = subdir_tasks
        .into_par_iter()
        .map(|(cname, cpath, cmtime, _dev, _ino)| {
            let cis_hid = cname.starts_with('.');
            walk_dir(cpath, cname, cmtime, cis_hid,
                     visited, cancel, stats, denom, tx.clone())
        })
        .collect();

    let mut children = file_nodes;
    children.extend(subdir_nodes);

    RawNode { name, size: 0, mtime, is_dir: true, is_hidden, children }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Walk `root` using `GetFileInformationByHandleEx` as the sole enumeration
/// primitive.
///
/// Returns `(arena, root_index)` ready for `finalize()` in `mod.rs`.
/// Progress callbacks are pumped on the caller's thread while the rayon walk
/// runs on a dedicated thread + pool, keeping the non-Send `on_progress`
/// closure off worker threads.
pub fn walk<F: FnMut(super::Progress)>(
    root:        PathBuf,
    cancel:      Arc<AtomicBool>,
    denom:       u64,
    mut on_progress: F,
) -> io::Result<(Vec<super::Node>, u32)> {
    // One metadata call for the root itself; all children come from bulk enumeration.
    let root_meta = std::fs::symlink_metadata(&root)?;
    let root_mtime = root_meta.modified().ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Seed visited with the root's (dev, ino) so junction loops back to the
    // root are detected. We re-use bulk_dir_meta on the root's parent to get
    // the FileId, but fall back to (0,0) if that's unavailable — not worth
    // failing the scan over.
    let root_dev_ino: (u64, u64) = {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
            FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_SHARE_DELETE,
            OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        };

        let wide: Vec<u16> = root.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0u16))
            .collect();
        let h = unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                0,
            )
        };
        if h == INVALID_HANDLE_VALUE {
            (0, 0)
        } else {
            let mut fi: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
            let ok = unsafe { GetFileInformationByHandle(h, &mut fi) };
            unsafe { CloseHandle(h) };
            if ok != 0 {
                let dev = fi.dwVolumeSerialNumber as u64;
                let ino = ((fi.nFileIndexHigh as u64) << 32) | (fi.nFileIndexLow as u64);
                (dev, ino)
            } else {
                (0, 0)
            }
        }
    };

    let visited: Arc<DashSet<(u64, u64)>> = Arc::new(DashSet::new());
    let stats:   Arc<WalkStats>           = Arc::new(WalkStats::default());

    if root_dev_ino != (0, 0) {
        visited.insert(root_dev_ino);
    }

    let (tx, rx) = std::sync::mpsc::channel::<Msg>();

    let root_name = root.to_string_lossy().into_owned();

    let root2    = root.clone();
    let cancel2  = Arc::clone(&cancel);
    let visited2 = Arc::clone(&visited);
    let stats2   = Arc::clone(&stats);
    let tx2      = tx.clone();

    // Build a dedicated rayon pool with an 8 MB stack per worker.
    // Windows default thread stack is 1 MB — deep trees need headroom.
    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8)
        .saturating_mul(2)
        .max(4);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(n_threads)
        .stack_size(8 * 1024 * 1024)
        .build()
        .unwrap_or_else(|_| rayon::ThreadPoolBuilder::new().build()
            .expect("rayon pool build failed"));

    // Spawn the walk on a dedicated OS thread so `on_progress` (FnMut, !Send)
    // stays on the caller thread.
    std::thread::spawn(move || {
        let root_node = pool.install(|| {
            walk_dir(
                root2, root_name, root_mtime, false,
                &visited2, &cancel2, &stats2, denom,
                tx2.clone(),
            )
        });
        let _ = tx2.send(Msg::Done(root_node));
    });

    // Pump progress on the caller thread until the walk completes.
    let root_raw = loop {
        match rx.recv() {
            Ok(Msg::Progress(p)) => on_progress(p),
            Ok(Msg::Done(raw))   => break raw,
            Err(_) => return Err(io::Error::new(
                io::ErrorKind::Other, "walk thread exited unexpectedly"
            )),
        }
    };

    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "scan cancelled"));
    }

    Ok(flatten(root_raw, &root))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::AtomicBool;

    /// Walk a small fixture tree and check structure.
    #[test]
    fn walk_basic_tree() {
        let root = std::env::temp_dir()
            .join(format!("diskviz_walk_win_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sub\\deep")).unwrap();
        fs::write(root.join("a.bin"),              vec![0u8; 4096]).unwrap();
        fs::write(root.join("sub\\b.bin"),         vec![0u8; 4096]).unwrap();
        fs::write(root.join("sub\\c.bin"),         vec![0u8; 4096]).unwrap();
        fs::write(root.join("sub\\deep\\d.bin"),   vec![0u8; 4096]).unwrap();
        fs::write(root.join(".hidden"),             vec![0u8; 4096]).unwrap();

        let cancel = Arc::new(AtomicBool::new(false));
        let (nodes, root_idx) = walk(root.clone(), cancel, 0, |_| {}).unwrap();

        // 5 files + root + sub + sub/deep = 8 nodes total.
        assert_eq!(nodes.len(), 8, "8 nodes total (3 dirs + 5 files)");
        assert_eq!(root_idx, 0,    "root always at index 0");

        let root_node = &nodes[0];
        assert!(root_node.is_dir);
        assert!(!root_node.is_hidden);

        let n_dirs  = nodes.iter().filter(|n| n.is_dir).count();
        let n_files = nodes.iter().filter(|n| !n.is_dir).count();
        assert_eq!(n_dirs,  3, "root + sub + sub/deep");
        assert_eq!(n_files, 5, "a.bin, b.bin, c.bin, d.bin, .hidden");

        let hidden = nodes.iter().find(|n| n.name == ".hidden").unwrap();
        assert!(hidden.is_hidden, ".hidden must be flagged is_hidden");

        let _ = fs::remove_dir_all(&root);
    }

    /// Cancellation flag causes the walk to return Interrupted.
    #[test]
    fn walk_cancel() {
        let root = std::env::temp_dir()
            .join(format!("diskviz_cancel_win_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("f.bin"), vec![0u8; 512]).unwrap();

        let cancel = Arc::new(AtomicBool::new(true)); // pre-cancelled
        match walk(root.clone(), cancel, 0, |_| {}) {
            Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::Interrupted,
                                 "cancelled walk must return Interrupted"),
            Ok(_)  => panic!("expected Interrupted error but walk succeeded"),
        }

        let _ = fs::remove_dir_all(&root);
    }
}
