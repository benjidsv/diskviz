//! macOS parallel directory walker using `getattrlistbulk` as the *primary*
//! enumeration primitive. Unlike the Phase-C approach (which ran getattrlistbulk
//! as a second pass alongside jwalk's readdir), this module replaces readdir
//! entirely — each directory is enumerated exactly once.
//!
//! `scan()` in `mod.rs` dispatches here on macOS unless `DISKVIZ_NO_BULK=1`.
//!
//! Shared scaffolding (`RawNode`, `WalkStats`, `Msg`, `emit_progress_if_due`,
//! `flatten`) lives in `walk_common` and is imported here.

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
/// must run this inside a `rayon::ThreadPool::install` scope so that nested
/// par_iters all dispatch to the same pool.
///
/// `visited` is shared across threads; it dedups hardlinked files (`nlink > 1`)
/// and prevents re-entering directory symlinks or hardlinked directories.
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

    // Try the bulk path first; fall back to readdir_meta on failure so we don't
    // drop whole subtrees the way jwalk wouldn't.
    let entries = match dirmeta::bulk_dir_meta(&path) {
        Some(e) => e,
        None => match dirmeta::readdir_meta(&path) {
            Some(e) => {
                stats.readdir_fallbacks.fetch_add(1, Ordering::Relaxed);
                e
            }
            None => {
                // Both failed — dir is genuinely unreadable; emit empty leaf.
                stats.open_failures.fetch_add(1, Ordering::Relaxed);
                stats.dir_count.fetch_add(1, Ordering::Relaxed);
                return RawNode { name, size: 0, mtime, is_dir: true, is_hidden, children: vec![] };
            }
        },
    };

    stats.dir_count.fetch_add(1, Ordering::Relaxed);

    let mut file_nodes:   Vec<RawNode>                     = Vec::new();
    let mut subdir_tasks: Vec<(String, PathBuf, i64, u64, u64)> = Vec::new();

    for (fname, rm) in entries.into_iter() {
        let is_child_hid = fname.starts_with('.');

        if rm.is_dir {
            if !visited.insert((rm.dev, rm.ino)) {
                // Already visited — emit as empty leaf to preserve the node in
                // the tree (for size accounting) but don't recurse.
                file_nodes.push(RawNode {
                    name: fname, size: 0, mtime: rm.mtime,
                    is_dir: true, is_hidden: is_child_hid, children: vec![],
                });
            } else {
                let subpath = path.join(&fname);
                subdir_tasks.push((fname, subpath, rm.mtime, rm.dev, rm.ino));
            }
        } else if rm.nlink > 1 && !visited.insert((rm.dev, rm.ino)) {
            // Hard link already counted — zero size to avoid double-counting.
            file_nodes.push(RawNode {
                name: fname, size: 0, mtime: rm.mtime,
                is_dir: false, is_hidden: is_child_hid, children: vec![],
            });
        } else {
            stats.file_count.fetch_add(1, Ordering::Relaxed);
            stats.bytes_scanned.fetch_add(rm.size, Ordering::Relaxed);
            file_nodes.push(RawNode {
                name: fname, size: rm.size, mtime: rm.mtime,
                is_dir: false, is_hidden: is_child_hid, children: vec![],
            });
        }
    }

    emit_progress_if_due(&path.to_string_lossy(), stats, denom, &tx);

    // Recurse into subdirectories in parallel. `par_iter` dispatches to the
    // ThreadPool established by `walk()` via `pool.install()`.
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

/// Walk `root` using `getattrlistbulk` as the sole enumeration primitive.
///
/// Returns `(arena, root_index)` ready for `finalize()` in `mod.rs`.
/// Progress callbacks are pumped on the caller's thread (the mpsc receiver loop)
/// while the rayon walk runs on a dedicated thread + pool, keeping the non-Send
/// `on_progress` closure off worker threads.
pub fn walk<F: FnMut(super::Progress)>(
    root:        PathBuf,
    cancel:      Arc<AtomicBool>,
    denom:       u64,
    mut on_progress: F,
) -> io::Result<(Vec<super::Node>, u32)> {
    use std::os::unix::fs::MetadataExt;

    // Raise the soft fd limit to the hard limit so getattrlistbulk can hold
    // many directory fds open simultaneously without hitting EMFILE.
    // On macOS the hard limit is typically 10240.
    unsafe {
        let mut rl: libc::rlimit = std::mem::zeroed();
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut rl) == 0 && rl.rlim_cur < rl.rlim_max {
            rl.rlim_cur = rl.rlim_max;
            let _ = libc::setrlimit(libc::RLIMIT_NOFILE, &rl);
        }
    }

    // One lstat for the root itself (all children come from bulk enumeration).
    let root_meta = std::fs::symlink_metadata(&root)?;
    let root_mtime = root_meta.modified().ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // dev_t on macOS is i32; zero-extend via u32 to match dirmeta::RawMeta::dev.
    let root_dev = root_meta.dev() as u32 as u64;
    let root_ino = root_meta.ino();

    let visited: Arc<DashSet<(u64, u64)>> = Arc::new(DashSet::new());
    let stats:   Arc<WalkStats>           = Arc::new(WalkStats::default());

    // Insert the scan root so any symlink looping back to it is detected.
    visited.insert((root_dev, root_ino));

    let (tx, rx) = std::sync::mpsc::channel::<Msg>();

    let root_name = root.to_string_lossy().into_owned();

    let root2    = root.clone();
    let cancel2  = Arc::clone(&cancel);
    let visited2 = Arc::clone(&visited);
    let stats2   = Arc::clone(&stats);
    let tx2      = tx.clone();

    // Build a dedicated rayon pool with an 8 MB stack per worker to safely
    // handle deeply-nested filesystem trees (default OS thread stack on macOS
    // is 512 KB for non-main threads).
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
    // stays on the caller thread. The pool is moved in; `pool.install` keeps it
    // alive until the walk closure finishes.
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

    // Optional diagnostics: set DISKVIZ_WALK_DIAG=1 to see fallback stats.
    if std::env::var_os("DISKVIZ_WALK_DIAG").is_some() {
        eprintln!(
            "[walk_macos] files={}  dirs={}  readdir_fallbacks={}  open_failures={}",
            stats.file_count.load(Ordering::Relaxed),
            stats.dir_count.load(Ordering::Relaxed),
            stats.readdir_fallbacks.load(Ordering::Relaxed),
            stats.open_failures.load(Ordering::Relaxed),
        );
    }

    Ok(flatten(root_raw, &root))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::Node;
    use std::fs;
    use std::sync::atomic::AtomicBool;

    /// Walk a small fixture tree and check that totals and dir/file structure
    /// match what a naïve recursive count would give.
    #[test]
    fn walk_basic_tree() {
        let root = std::env::temp_dir()
            .join(format!("diskviz_walk_macos_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sub/deep")).unwrap();
        fs::write(root.join("a.bin"),          vec![0u8; 4096]).unwrap();
        fs::write(root.join("sub/b.bin"),      vec![0u8; 4096]).unwrap();
        fs::write(root.join("sub/c.bin"),      vec![0u8; 4096]).unwrap();
        fs::write(root.join("sub/deep/d.bin"), vec![0u8; 4096]).unwrap();
        // Hidden file
        fs::write(root.join(".hidden"),        vec![0u8; 4096]).unwrap();

        let cancel = Arc::new(AtomicBool::new(false));
        let (nodes, root_idx) = walk(root.clone(), cancel, 0, |_| {}).unwrap();

        // Before finalize(), children list is correct but sizes not aggregated.
        // Check counts: 5 files + root + sub + sub/deep = 8 nodes total.
        assert_eq!(nodes.len(), 8, "8 nodes total (3 dirs + 5 files)");
        assert_eq!(root_idx, 0, "root always at index 0");

        let root_node = &nodes[0];
        assert!(root_node.is_dir);
        assert!(!root_node.is_hidden);

        // Count files and dirs across all nodes.
        let n_dirs  = nodes.iter().filter(|n| n.is_dir).count();
        let n_files = nodes.iter().filter(|n| !n.is_dir).count();
        assert_eq!(n_dirs,  3, "root + sub + sub/deep");
        assert_eq!(n_files, 5, "a.bin, b.bin, c.bin, d.bin, .hidden");

        let hidden = nodes.iter().find(|n| n.name == ".hidden").unwrap();
        assert!(hidden.is_hidden);

        let _ = fs::remove_dir_all(&root);
    }

    /// Hardlinked file appears once (size counted) and once as zero-size duplicate.
    #[test]
    fn walk_hardlink_dedup() {
        let root = std::env::temp_dir()
            .join(format!("diskviz_hl_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let orig = root.join("orig.txt");
        fs::write(&orig, vec![0u8; 8192]).unwrap();
        // Create a hard link inside the same dir.
        fs::hard_link(&orig, root.join("link.txt")).unwrap();

        let cancel = Arc::new(AtomicBool::new(false));
        let (nodes, _) = walk(root.clone(), cancel, 0, |_| {}).unwrap();

        let files: Vec<&Node> = nodes.iter().filter(|n| !n.is_dir).collect();
        assert_eq!(files.len(), 2, "both entries visible");
        // Exactly one of them has size > 0.
        let sizes: Vec<u64> = files.iter().map(|n| n.size).collect();
        let nonzero = sizes.iter().filter(|&&s| s > 0).count();
        assert_eq!(nonzero, 1, "only one hardlink copy is counted");

        let _ = fs::remove_dir_all(&root);
    }

    /// Cancellation flag causes the walk to return Interrupted.
    #[test]
    fn walk_cancel() {
        let root = std::env::temp_dir()
            .join(format!("diskviz_cancel_{}", std::process::id()));
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
