//! Fast parallel filesystem scanner.
//!
//! Walks a directory tree in parallel and stores the result in a compact
//! index-based arena (`Vec<Node>`). Directory sizes are aggregated bottom-up in
//! a single reverse pass. Subtree extension/age stats are precomputed bottom-up
//! (Phase A) so navigation lookups are O(1). Children are sorted by size in
//! parallel via rayon (Phase B).
//!
//! On macOS (unless `DISKVIZ_NO_BULK=1` is set), `walk_macos` replaces jwalk
//! entirely, using `getattrlistbulk` as the sole enumeration primitive — each
//! directory is read once rather than twice. On all other platforms jwalk is
//! used with per-entry `metadata()` calls.

pub mod dirmeta;
pub mod walk_common;
#[cfg(target_os = "macos")]
mod walk_macos;
#[cfg(target_os = "windows")]
mod walk_windows;

// ── Walker selection ──────────────────────────────────────────────────────────

/// Identifies which enumeration back-end to use for a scan.
///
/// `scan()` (the public API used by the Tauri commands) always uses
/// `Walker::Default`. Tests and the comparison harness can pass a specific
/// variant to `scan_with()` to request a particular path without resorting
/// to env-var juggling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Walker {
    /// Today's behaviour: platform fast-path (custom) unless `DISKVIZ_NO_BULK=1`,
    /// in which case the jwalk fallback is used.
    Default,
    /// Always use the platform-native fast walker (`walk_macos` / `walk_windows`).
    Custom,
    /// Always use the jwalk fallback (env-var-free).
    Jwalk,
    // Mft variant will be added here when the MFT/WizTree-style walker lands.
}

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use jwalk::WalkDirGeneric;
use rayon::prelude::*;

// ── Shared age/extension helpers (also used by commands.rs) ──────────────────

/// Lowercased extension (no dot) for a file name; empty if it has none.
pub fn extension_of(name: &str) -> String {
    std::path::Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Upper bounds (exclusive, days) of each age bucket; last bucket is open-ended.
pub const AGE_BUCKET_DAYS: [i64; 8] = [1, 7, 30, 90, 180, 365, 730, 1825];
/// Representative age (days) reported for files landing in each bucket.
pub const AGE_BUCKET_REP_DAYS: [i64; 9] = [0, 3, 18, 60, 135, 270, 545, 1277, 2555];
pub const SECS_PER_DAY: i64 = 86_400;

pub fn age_bucket(age_days: i64) -> usize {
    AGE_BUCKET_DAYS
        .iter()
        .position(|&b| age_days < b)
        .unwrap_or(AGE_BUCKET_DAYS.len())
}

// ── Precomputed subtree stats ─────────────────────────────────────────────────

/// Aggregated file-type and age data for one directory node, computed once at
/// scan time during the bottom-up stats reverse pass.
pub struct DirStats {
    /// Age histogram: count of file descendants in each bucket.
    pub age_hist: [u32; 9],
    /// Extensions sorted descending by total size; IDs map into `ScanTree::ext_names`.
    pub exts: Vec<(u32, u64)>,
}

// ── Per-entry metadata ────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct EntryMeta {
    size: u64,
    mtime: i64,
}

// ── Arena node ────────────────────────────────────────────────────────────────

pub struct Node {
    pub name: String,
    pub size: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub is_dir: bool,
    pub is_hidden: bool,
    pub mtime: i64,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
}

// ── ScanTree ──────────────────────────────────────────────────────────────────

pub struct ScanTree {
    pub nodes: Vec<Node>,
    pub root: u32,
    pub root_path: PathBuf,
    pub total_size: u64,
    pub total_files: u64,
    pub total_dirs: u64,
    pub scan_duration_ms: u64,
    /// Extension interner: ext_id → lowercased extension string (no dot).
    pub ext_names: Vec<String>,
    /// Precomputed subtree stats keyed by directory node index.
    pub dir_stats: HashMap<u32, DirStats>,
    /// Unix-second timestamp captured at scan time. Phase A and `remove_subtree`
    /// both use this clock so age-bucket assignments stay consistent.
    pub scan_now_secs: i64,
}

impl ScanTree {
    pub fn path_of(&self, mut idx: u32) -> PathBuf {
        let mut parts: Vec<&str> = Vec::new();
        loop {
            let node = &self.nodes[idx as usize];
            match node.parent {
                Some(p) => {
                    parts.push(&node.name);
                    idx = p;
                }
                None => break,
            }
        }
        let mut path = self.root_path.clone();
        for part in parts.iter().rev() {
            path.push(part);
        }
        path
    }

    /// Remove a node (and its whole subtree) from the arena. Subtracts
    /// size/counts and DirStats from all ancestors, detaches from parent,
    /// and tombstones the node. Returns updated tree totals.
    pub fn remove_subtree(&mut self, idx: u32) -> (u64, u64, u64) {
        let node = &self.nodes[idx as usize];
        let d_size  = node.size;
        let d_files = node.file_count + if node.is_dir { 0 } else { 1 };
        let d_dirs  = node.dir_count  + if node.is_dir { 1 } else { 0 };
        let parent_opt = node.parent;

        // Build the DirStats delta to subtract from ancestors.
        let del_stats: Option<DirStats> = if node.is_dir {
            self.dir_stats.remove(&idx)
        } else {
            let ext = extension_of(&node.name);
            let ext_id = self.ext_names.iter().position(|e| e == &ext).map(|i| i as u32);
            // Use the scan-time clock so the bucket matches what Phase A assigned.
            let age_days = (self.scan_now_secs - node.mtime).max(0) / SECS_PER_DAY;
            let bucket = age_bucket(age_days);
            let mut ds = DirStats { age_hist: [0u32; 9], exts: Vec::new() };
            ds.age_hist[bucket] = 1;
            if let Some(eid) = ext_id {
                ds.exts.push((eid, node.size));
            }
            Some(ds)
        };

        // Subtract size/count from every ancestor.
        let mut cur_opt = parent_opt;
        while let Some(cur) = cur_opt {
            let anc = &mut self.nodes[cur as usize];
            anc.size       = anc.size.saturating_sub(d_size);
            anc.file_count = anc.file_count.saturating_sub(d_files);
            anc.dir_count  = anc.dir_count.saturating_sub(d_dirs);
            cur_opt = anc.parent;
        }

        // Subtract DirStats from every ancestor.
        if let Some(ref ds) = del_stats {
            let mut cur_opt = parent_opt;
            while let Some(cur) = cur_opt {
                if let Some(anc_stats) = self.dir_stats.get_mut(&cur) {
                    for b in 0..9 {
                        anc_stats.age_hist[b] =
                            anc_stats.age_hist[b].saturating_sub(ds.age_hist[b]);
                    }
                    for &(del_eid, del_sz) in &ds.exts {
                        if let Some(pos) =
                            anc_stats.exts.iter().position(|&(eid, _)| eid == del_eid)
                        {
                            if anc_stats.exts[pos].1 <= del_sz {
                                anc_stats.exts.remove(pos);
                            } else {
                                anc_stats.exts[pos].1 -= del_sz;
                            }
                        }
                    }
                    anc_stats.exts.sort_unstable_by(|a, b| b.1.cmp(&a.1));
                }
                cur_opt = self.nodes[cur as usize].parent;
            }
        }

        // Detach from parent's children list.
        if let Some(p) = parent_opt {
            self.nodes[p as usize].children.retain(|&c| c != idx);
        }

        // Tombstone.
        let node = &mut self.nodes[idx as usize];
        node.size       = 0;
        node.file_count = 0;
        node.dir_count  = 0;
        node.children.clear();

        self.total_size  = self.total_size.saturating_sub(d_size);
        self.total_files = self.total_files.saturating_sub(d_files);
        self.total_dirs  = self.total_dirs.saturating_sub(d_dirs);

        (self.total_size, self.total_files, self.total_dirs)
    }
}

// ── Progress ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Progress {
    pub current_path: String,
    pub files_scanned: u64,
    pub directories_scanned: u64,
    pub bytes_scanned: u64,
    pub percent: f64,
}

// ── finalize() ────────────────────────────────────────────────────────────────
//
// Shared tail for both the macOS walker and the jwalk path: bottom-up size
// aggregation, Phase A extension/age stats, Phase B parallel child-sort, and
// ScanTree assembly.

fn finalize(
    mut nodes:     Vec<Node>,
    root_index:    u32,
    root_path:     PathBuf,
    start:         Instant,
    scan_now_secs: i64,
) -> ScanTree {
    // ── Bottom-up size aggregation ────────────────────────────────────────────
    for i in (0..nodes.len()).rev() {
        if let Some(p) = nodes[i].parent {
            let s = nodes[i].size;
            let (fc, dc, is_dir) = (nodes[i].file_count, nodes[i].dir_count, nodes[i].is_dir);
            let parent = &mut nodes[p as usize];
            parent.size       += s;
            parent.file_count += fc + if is_dir { 0 } else { 1 };
            parent.dir_count  += dc + if is_dir { 1 } else { 0 };
        }
    }

    // ── Phase A: bottom-up stats reverse pass ─────────────────────────────────
    // Build extension interner + per-dir accumulator. Same child > parent index
    // invariant as the size pass: processing in reverse finishes every child
    // before its parent is reached.
    struct DirAccum {
        exts:     HashMap<u32, u64>,
        age_hist: [u64; 9],
    }

    let mut ext_map:   HashMap<String, u32> = HashMap::new();
    let mut ext_names: Vec<String>          = Vec::new();
    // Only dir nodes get Some; files stay None.
    let mut accum: Vec<Option<DirAccum>> = nodes
        .iter()
        .map(|n| if n.is_dir {
            Some(DirAccum { exts: HashMap::new(), age_hist: [0; 9] })
        } else {
            None
        })
        .collect();

    for i in (0..nodes.len()).rev() {
        let p_opt = nodes[i].parent;
        if !nodes[i].is_dir {
            // File: fold ext + age into parent directory's accumulator.
            let ext_str = extension_of(&nodes[i].name);
            let n_ext   = ext_names.len() as u32;
            let ext_id  = *ext_map.entry(ext_str.clone()).or_insert_with(|| {
                ext_names.push(ext_str);
                n_ext
            });
            let age_days = (scan_now_secs - nodes[i].mtime).max(0) / SECS_PER_DAY;
            let bucket   = age_bucket(age_days);
            let sz       = nodes[i].size;
            if let Some(p) = p_opt {
                if let Some(Some(pa)) = accum.get_mut(p as usize) {
                    *pa.exts.entry(ext_id).or_insert(0) += sz;
                    pa.age_hist[bucket] += 1;
                }
            }
        } else if let Some(p) = p_opt {
            // Dir: merge its completed accum into parent. p < i guaranteed,
            // so split_at_mut yields non-overlapping borrows.
            let (left, right) = accum.split_at_mut(i);
            if let (Some(child_a), Some(Some(parent_a))) =
                (right[0].as_ref(), left.get_mut(p as usize))
            {
                for (&eid, &sz) in &child_a.exts {
                    *parent_a.exts.entry(eid).or_insert(0) += sz;
                }
                for b in 0..9 { parent_a.age_hist[b] += child_a.age_hist[b]; }
            }
        }
    }

    // Convert accumulators to sorted DirStats.
    let mut dir_stats: HashMap<u32, DirStats> = HashMap::new();
    for (i, acc_opt) in accum.into_iter().enumerate() {
        if let Some(acc) = acc_opt {
            let mut exts: Vec<(u32, u64)> = acc.exts.into_iter().collect();
            exts.sort_unstable_by(|a, b| b.1.cmp(&a.1));
            dir_stats.insert(i as u32, DirStats {
                age_hist: acc.age_hist.map(|x| x as u32),
                exts,
            });
        }
    }

    // ── Phase B: parallel child-sort ─────────────────────────────────────────
    let sizes: Vec<u64> = nodes.iter().map(|n| n.size).collect();
    nodes.par_iter_mut().for_each(|n| {
        n.children.sort_unstable_by(|&a, &b| sizes[b as usize].cmp(&sizes[a as usize]));
    });

    let total_size  = nodes[root_index as usize].size;
    let total_files = nodes[root_index as usize].file_count;
    let total_dirs  = nodes[root_index as usize].dir_count;

    ScanTree {
        nodes,
        root: root_index,
        root_path,
        total_size,
        total_files,
        total_dirs,
        scan_duration_ms: start.elapsed().as_millis() as u64,
        ext_names,
        dir_stats,
        scan_now_secs,
    }
}

// ── scan() / scan_with() ─────────────────────────────────────────────────────

/// Scan `root` with a specific walker back-end. This is the testable seam;
/// `scan()` delegates here with `Walker::Default`.
pub fn scan_with<F: FnMut(Progress)>(
    root: PathBuf,
    cancel: Arc<AtomicBool>,
    mut on_progress: F,
    walker: Walker,
) -> std::io::Result<ScanTree> {
    let start = Instant::now();
    let denom = volume_used_bytes(&root).unwrap_or(0);

    let scan_now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let use_custom = match walker {
        Walker::Custom  => true,
        Walker::Jwalk   => false,
        Walker::Default => std::env::var_os("DISKVIZ_NO_BULK").is_none(),
    };

    // ── macOS fast path ───────────────────────────────────────────────────────
    #[cfg(target_os = "macos")]
    if use_custom {
        let (nodes, root_index) =
            walk_macos::walk(root.clone(), Arc::clone(&cancel), denom, &mut on_progress)?;
        return Ok(finalize(nodes, root_index, root, start, scan_now_secs));
    }

    // ── Windows fast path ─────────────────────────────────────────────────────
    #[cfg(target_os = "windows")]
    if use_custom {
        let (nodes, root_index) =
            walk_windows::walk(root.clone(), Arc::clone(&cancel), denom, &mut on_progress)?;
        return Ok(finalize(nodes, root_index, root, start, scan_now_secs));
    }

    // Suppress unused-variable warning on non-macOS, non-Windows builds.
    let _ = use_custom;

    // ── jwalk path ────────────────────────────────────────────────────────────
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8)
        .saturating_mul(2)
        .max(4);

    #[cfg(unix)]
    let visited: dashmap::DashSet<(u64, u64)> = dashmap::DashSet::new();

    let cancel_walk = cancel.clone();
    let walk = WalkDirGeneric::<((), EntryMeta)>::new(&root)
        .skip_hidden(false)
        .follow_links(false)
        .parallelism(jwalk::Parallelism::RayonNewPool(threads))
        .process_read_dir(move |_depth, _path, _read_dir_state, children| {
            if cancel_walk.load(Ordering::Relaxed) {
                children.clear();
                return;
            }

            for entry in children.iter_mut().flatten() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;

                    if let Ok(meta) = entry.metadata() {
                        let mtime = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);
                        let is_dir = meta.is_dir();
                        let size   = meta.blocks().saturating_mul(512);

                        if is_dir || meta.nlink() > 1 {
                            if !visited.insert((meta.dev(), meta.ino())) {
                                entry.client_state = EntryMeta { size: 0, mtime };
                                if is_dir { entry.read_children_path = None; }
                                continue;
                            }
                        }
                        entry.client_state = EntryMeta { size, mtime };
                    }
                }

                #[cfg(not(unix))]
                if let Ok(meta) = entry.metadata() {
                    let mtime = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    entry.client_state = EntryMeta { size: meta.len(), mtime };
                }
            }
        });

    let mut nodes: Vec<Node> = Vec::new();
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
        let depth  = entry.depth();
        let is_dir = entry.file_type().is_dir();
        let meta   = entry.client_state.clone();

        let name = if depth == 0 {
            entry.path().to_string_lossy().to_string()
        } else {
            entry.file_name().to_string_lossy().to_string()
        };
        let is_hidden = depth != 0 && name.starts_with('.');

        let idx    = nodes.len() as u32;
        let parent = if depth == 0 { None } else { parent_at_depth.get(depth - 1).copied() };

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

        if let Some(p) = parent { nodes[p as usize].children.push(idx); }

        if parent_at_depth.len() <= depth { parent_at_depth.resize(depth + 1, 0); }
        parent_at_depth[depth] = idx;

        if depth == 0 { root_idx = Some(idx); }

        if is_dir { total_dirs += 1; } else { total_files += 1; bytes_scanned += meta.size; }

        since_emit += 1;
        if since_emit >= 4096 || last_emit.elapsed().as_millis() >= 80 {
            since_emit = 0;
            last_emit = Instant::now();
            let percent = if denom > 0 {
                ((bytes_scanned as f64 / denom as f64) * 100.0).min(99.0)
            } else {
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

    Ok(finalize(nodes, root_index, root, start, scan_now_secs))
}

/// Scan `root` with the default walker strategy (env-var–aware fast path).
/// This is the public API used by all Tauri commands.
pub fn scan<F: FnMut(Progress)>(
    root: PathBuf,
    cancel: Arc<AtomicBool>,
    on_progress: F,
) -> std::io::Result<ScanTree> {
    scan_with(root, cancel, on_progress, Walker::Default)
}

// ── volume_used_bytes ─────────────────────────────────────────────────────────

#[cfg(unix)]
fn volume_used_bytes(path: &std::path::Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut stat) != 0 { return None; }
        let frsize = stat.f_frsize as u64;
        let used_blocks = stat.f_blocks.saturating_sub(stat.f_bfree) as u64;
        Some(used_blocks.saturating_mul(frsize))
    }
}

#[cfg(not(unix))]
fn volume_used_bytes(_path: &std::path::Path) -> Option<u64> { None }

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // ── Existing integration tests ──────────────────────────────────────────

    #[test]
    fn aggregates_sizes_and_counts() {
        let root = std::env::temp_dir().join(format!("diskviz_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sub/deep")).unwrap();
        fs::write(root.join("a.bin"),          vec![0u8; 100]).unwrap();
        fs::write(root.join("sub/b.bin"),      vec![0u8; 200]).unwrap();
        fs::write(root.join("sub/c.bin"),      vec![0u8; 300]).unwrap();
        fs::write(root.join("sub/deep/d.bin"), vec![0u8; 400]).unwrap();

        let cancel = Arc::new(AtomicBool::new(false));
        let tree = scan(root.clone(), cancel, |_| {}).unwrap();

        assert_eq!(tree.total_files, 4, "four files");
        assert_eq!(tree.total_dirs,  2, "sub and sub/deep");
        assert!(tree.total_size >= 1000, "allocated >= logical bytes");

        let root_node = &tree.nodes[tree.root as usize];
        assert_eq!(root_node.size,       tree.total_size);
        assert_eq!(root_node.file_count, 4);
        assert_eq!(root_node.dir_count,  2);

        let sub = &tree.nodes[root_node.children[0] as usize];
        assert_eq!(sub.name, "sub");
        assert!(sub.size >= 900);
        assert_eq!(sub.file_count, 3);
        assert_eq!(sub.dir_count,  1);

        // Phase A: dir_stats correctness.
        let root_stats = tree.dir_stats.get(&tree.root).expect("root has dir_stats");
        let hist_total: u32 = root_stats.age_hist.iter().sum();
        assert_eq!(hist_total, 4, "age_hist covers all 4 files");
        let bin_id = tree.ext_names.iter().position(|e| e == "bin")
            .expect(".bin interned") as u32;
        assert!(root_stats.exts.iter().any(|&(eid, _)| eid == bin_id), ".bin in exts");

        let _ = fs::remove_dir_all(&root);
    }

    /// `remove_subtree` must decrement the exact age bucket that Phase A used
    /// (scan-time clock), not a freshly computed "now" bucket.
    #[test]
    fn remove_subtree_bucket_consistency() {
        let root = std::env::temp_dir()
            .join(format!("diskviz_rm_bucket_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("target.bin"), vec![0u8; 4096]).unwrap();

        let cancel = Arc::new(AtomicBool::new(false));
        let mut tree = scan(root.clone(), cancel, |_| {}).unwrap();

        let root_node = &tree.nodes[tree.root as usize];
        // Find the file node index.
        let file_idx = *root_node.children.iter()
            .find(|&&c| !tree.nodes[c as usize].is_dir)
            .expect("file child exists");

        // Record the histogram before deletion.
        let hist_before = tree.dir_stats.get(&tree.root)
            .expect("root dir_stats")
            .age_hist;

        // Delete the file from the arena (does NOT touch the filesystem).
        tree.remove_subtree(file_idx);

        let hist_after = tree.dir_stats.get(&tree.root)
            .expect("root dir_stats still present")
            .age_hist;

        // Exactly one bucket must have decremented by 1; the rest stay the same.
        let diffs: Vec<i64> = hist_before.iter()
            .zip(hist_after.iter())
            .map(|(&b, &a)| b as i64 - a as i64)
            .collect();
        let decremented: Vec<_> = diffs.iter().enumerate().filter(|&(_, &d)| d != 0).collect();

        assert_eq!(decremented.len(), 1, "exactly one bucket changed");
        assert_eq!(decremented[0].1, &1i64, "bucket decremented by 1");

        let _ = fs::remove_dir_all(&root);
    }

    /// macOS parity: scanning with DISKVIZ_NO_BULK (jwalk path) and without
    /// (walk_macos path) should yield the same total sizes, counts, and
    /// dir_stats histograms.
    #[test]
    #[cfg(target_os = "macos")]
    fn macos_walker_parity() {
        let root = std::env::temp_dir()
            .join(format!("diskviz_parity_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sub/deep")).unwrap();
        fs::write(root.join("a.rs"),           vec![0u8; 4096]).unwrap();
        fs::write(root.join("sub/b.toml"),     vec![0u8; 4096]).unwrap();
        fs::write(root.join("sub/c.toml"),     vec![0u8; 4096]).unwrap();
        fs::write(root.join("sub/deep/d.rs"),  vec![0u8; 4096]).unwrap();
        fs::write(root.join(".hidden"),        vec![0u8; 4096]).unwrap();

        let cancel1 = Arc::new(AtomicBool::new(false));
        let cancel2 = Arc::new(AtomicBool::new(false));

        // Walk via walk_macos path (Walker::Custom).
        let fast = scan_with(root.clone(), cancel1, |_| {}, Walker::Custom).unwrap();

        // Walk via jwalk fallback (Walker::Jwalk — no env-var needed).
        let slow = scan_with(root.clone(), cancel2, |_| {}, Walker::Jwalk).unwrap();

        assert_eq!(fast.total_files, slow.total_files, "file count");
        assert_eq!(fast.total_dirs,  slow.total_dirs,  "dir count");
        assert_eq!(fast.total_size,  slow.total_size,  "total size");

        // Root histogram totals must match.
        let fh: u32 = fast.dir_stats.get(&fast.root).unwrap().age_hist.iter().sum();
        let sh: u32 = slow.dir_stats.get(&slow.root).unwrap().age_hist.iter().sum();
        assert_eq!(fh, sh, "age_hist total (file count) matches");

        let _ = fs::remove_dir_all(&root);
    }

    // ── New unit tests for pure helpers ────────────────────────────────────

    // ── extension_of ───────────────────────────────────────────────────────

    #[test]
    fn ext_no_extension() {
        assert_eq!(extension_of("README"), "");
        assert_eq!(extension_of(""), "");
    }

    #[test]
    fn ext_multiple_dots() {
        // Only the final component after the last dot is the extension.
        assert_eq!(extension_of("archive.tar.gz"), "gz");
    }

    #[test]
    fn ext_dotfile_has_no_extension() {
        // A file whose name starts with a dot and has no further dot is a dotfile,
        // not a file with extension "gitignore".
        assert_eq!(extension_of(".gitignore"), "");
        assert_eq!(extension_of(".hidden"), "");
    }

    #[test]
    fn ext_uppercased_becomes_lowercase() {
        assert_eq!(extension_of("Image.PNG"), "png");
        assert_eq!(extension_of("Movie.MOV"), "mov");
    }

    #[test]
    fn ext_trailing_dot() {
        // A trailing dot means the extension is an empty string.
        assert_eq!(extension_of("file."), "");
    }

    // ── age_bucket ─────────────────────────────────────────────────────────

    #[test]
    fn age_bucket_lengths_and_monotonic() {
        assert_eq!(AGE_BUCKET_DAYS.len(), 8);
        assert_eq!(AGE_BUCKET_REP_DAYS.len(), 9);
        // Boundaries must be strictly increasing.
        for w in AGE_BUCKET_DAYS.windows(2) {
            assert!(w[0] < w[1], "AGE_BUCKET_DAYS must be monotonically increasing");
        }
        for w in AGE_BUCKET_REP_DAYS.windows(2) {
            assert!(w[0] < w[1], "AGE_BUCKET_REP_DAYS must be monotonically increasing");
        }
    }

    #[test]
    fn age_bucket_each_boundary() {
        // age_days < 1 → bucket 0
        assert_eq!(age_bucket(0), 0);
        // age_days == boundary → falls into the NEXT bucket (position finds first b > age)
        assert_eq!(age_bucket(1),    1, "age=1 → bucket 1");
        assert_eq!(age_bucket(7),    2, "age=7 → bucket 2");
        assert_eq!(age_bucket(30),   3, "age=30 → bucket 3");
        assert_eq!(age_bucket(90),   4, "age=90 → bucket 4");
        assert_eq!(age_bucket(180),  5, "age=180 → bucket 5");
        assert_eq!(age_bucket(365),  6, "age=365 → bucket 6");
        assert_eq!(age_bucket(730),  7, "age=730 → bucket 7");
        assert_eq!(age_bucket(1825), 8, "age=1825 → bucket 8 (open-ended)");
        // Values inside each bucket's range.
        assert_eq!(age_bucket(6),    1, "age=6 < 7 → bucket 1");
        assert_eq!(age_bucket(29),   2, "age=29 < 30 → bucket 2");
    }

    #[test]
    fn age_bucket_negative_age() {
        // Negative age (future mtime) → bucket 0.
        assert_eq!(age_bucket(-1), 0);
        assert_eq!(age_bucket(-999), 0);
    }

    #[test]
    fn age_bucket_huge_age() {
        assert_eq!(age_bucket(9999), 8, "very old → last (open-ended) bucket");
        assert_eq!(age_bucket(i64::MAX / 2), 8);
    }

    // ── remove_subtree ancestor propagation ───────────────────────────────

    /// Build a minimal 3-level arena by hand and verify that remove_subtree
    /// correctly subtracts size, file_count, and dir_count from ALL ancestors,
    /// and removes the extension entry from all ancestor DirStats.
    #[test]
    fn remove_subtree_ancestor_propagation() {
        // Layout:
        //   root  (idx 0, dir)
        //     └── mid   (idx 1, dir)
        //           └── leaf  (idx 2, file, .rs, size 1000)

        let nodes = vec![
            Node { name: "/root".into(), size: 1000, file_count: 1, dir_count: 1,
                   is_dir: true, is_hidden: false, mtime: 0, parent: None, children: vec![1] },
            Node { name: "mid".into(),  size: 1000, file_count: 1, dir_count: 0,
                   is_dir: true, is_hidden: false, mtime: 0, parent: Some(0), children: vec![2] },
            Node { name: "leaf.rs".into(), size: 1000, file_count: 0, dir_count: 0,
                   is_dir: false, is_hidden: false, mtime: 0, parent: Some(1), children: vec![] },
        ];

        let ext_names = vec!["rs".to_string()];
        let mut dir_stats = std::collections::HashMap::new();
        // Build minimal DirStats: root and mid each have .rs with 1000 bytes, 1 file in bucket 0.
        dir_stats.insert(0u32, DirStats { age_hist: [1,0,0,0,0,0,0,0,0], exts: vec![(0,1000)] });
        dir_stats.insert(1u32, DirStats { age_hist: [1,0,0,0,0,0,0,0,0], exts: vec![(0,1000)] });

        let mut tree = ScanTree {
            nodes,
            root: 0,
            root_path: std::path::PathBuf::from("/root"),
            total_size: 1000,
            total_files: 1,
            total_dirs: 1,
            scan_duration_ms: 0,
            ext_names,
            dir_stats,
            scan_now_secs: 0,
        };

        tree.remove_subtree(2); // delete leaf.rs

        // Size/count must drop to zero for all nodes up the chain.
        assert_eq!(tree.nodes[0].size,       0, "root size zeroed");
        assert_eq!(tree.nodes[0].file_count, 0, "root file_count zeroed");
        // Deleting a file node does NOT decrement dir_count (d_dirs = 0 for files).
        // root still has dir_count=1 (the 'mid' directory is unchanged).
        assert_eq!(tree.nodes[0].dir_count,  1, "root dir_count unchanged (mid dir still there)");
        assert_eq!(tree.nodes[1].size,       0, "mid size zeroed");
        assert_eq!(tree.nodes[1].file_count, 0, "mid file_count zeroed");

        // mid must no longer be in root's children.
        // leaf must no longer be in mid's children.
        assert!(!tree.nodes[1].children.contains(&2), "leaf detached from mid");

        // DirStats: the .rs ext must have been removed from both root and mid.
        assert!(tree.dir_stats[&0].exts.is_empty(), "root exts cleared");
        assert!(tree.dir_stats[&1].exts.is_empty(), "mid exts cleared");
        assert_eq!(tree.dir_stats[&0].age_hist[0], 0, "root bucket 0 decremented");
        assert_eq!(tree.dir_stats[&1].age_hist[0], 0, "mid bucket 0 decremented");
    }

    #[test]
    fn remove_subtree_saturating_sub_guard() {
        // Verify that underflowing counts saturate to 0 rather than panicking
        // (the implementation uses saturating_sub everywhere).
        let nodes = vec![
            Node { name: "root".into(), size: 0, file_count: 0, dir_count: 0,
                   is_dir: true, is_hidden: false, mtime: 0, parent: None, children: vec![1] },
            Node { name: "file.bin".into(), size: 500, file_count: 0, dir_count: 0,
                   is_dir: false, is_hidden: false, mtime: 0, parent: Some(0), children: vec![] },
        ];
        let ext_names = vec!["bin".to_string()];
        let mut dir_stats = std::collections::HashMap::new();
        // Intentionally incorrect stats (0 rather than 1) to trigger the underflow path.
        dir_stats.insert(0u32, DirStats { age_hist: [0;9], exts: vec![(0, 500)] });
        let mut tree = ScanTree {
            nodes, root: 0,
            root_path: std::path::PathBuf::from("."),
            total_size: 0, total_files: 0, total_dirs: 0,
            scan_duration_ms: 0,
            ext_names, dir_stats, scan_now_secs: 0,
        };
        // Should not panic.
        tree.remove_subtree(1);
        assert_eq!(tree.nodes[0].size, 0);
    }

    // ── path_of ────────────────────────────────────────────────────────────

    #[test]
    fn path_of_reconstructs_path() {
        // Three-level tree: root → sub → file
        let root_path = std::path::PathBuf::from("/base");
        let nodes = vec![
            Node { name: "/base".into(), size: 0, file_count: 0, dir_count: 0,
                   is_dir: true, is_hidden: false, mtime: 0, parent: None, children: vec![1] },
            Node { name: "sub".into(), size: 0, file_count: 0, dir_count: 0,
                   is_dir: true, is_hidden: false, mtime: 0, parent: Some(0), children: vec![2] },
            Node { name: "file.txt".into(), size: 0, file_count: 0, dir_count: 0,
                   is_dir: false, is_hidden: false, mtime: 0, parent: Some(1), children: vec![] },
        ];
        let tree = ScanTree {
            nodes,
            root: 0,
            root_path: root_path.clone(),
            total_size: 0, total_files: 0, total_dirs: 0,
            scan_duration_ms: 0,
            ext_names: vec![],
            dir_stats: std::collections::HashMap::new(),
            scan_now_secs: 0,
        };

        assert_eq!(tree.path_of(0), root_path, "root → root_path");
        assert_eq!(tree.path_of(1), root_path.join("sub"), "sub dir");
        assert_eq!(tree.path_of(2), root_path.join("sub").join("file.txt"), "nested file");
    }

    // ── Windows parity test ────────────────────────────────────────────────

    /// Windows parity: the custom GetFileInformationByHandleEx walker and the
    /// jwalk fallback must yield identical file counts, dir counts, and total
    /// sizes on the same fixture tree.
    #[test]
    #[cfg(target_os = "windows")]
    fn windows_walker_parity() {
        let root = std::env::temp_dir()
            .join(format!("diskviz_win_parity_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sub\\deep")).unwrap();
        fs::write(root.join("a.rs"),              vec![0u8; 4096]).unwrap();
        fs::write(root.join("sub\\b.toml"),       vec![0u8; 4096]).unwrap();
        fs::write(root.join("sub\\c.toml"),       vec![0u8; 4096]).unwrap();
        fs::write(root.join("sub\\deep\\d.rs"),   vec![0u8; 4096]).unwrap();
        fs::write(root.join(".hidden"),            vec![0u8; 4096]).unwrap();

        let cancel1 = Arc::new(AtomicBool::new(false));
        let cancel2 = Arc::new(AtomicBool::new(false));

        // Custom path: GetFileInformationByHandleEx walker.
        let fast = scan_with(root.clone(), cancel1, |_| {}, Walker::Custom).unwrap();
        // jwalk fallback (no env-var juggling).
        let slow = scan_with(root.clone(), cancel2, |_| {}, Walker::Jwalk).unwrap();

        assert_eq!(fast.total_files, slow.total_files, "file count");
        assert_eq!(fast.total_dirs,  slow.total_dirs,  "dir count");
        assert_eq!(fast.total_size,  slow.total_size,  "total size");

        let fh: u32 = fast.dir_stats.get(&fast.root).unwrap().age_hist.iter().sum();
        let sh: u32 = slow.dir_stats.get(&slow.root).unwrap().age_hist.iter().sum();
        assert_eq!(fh, sh, "age_hist total (file count) matches");

        let _ = fs::remove_dir_all(&root);
    }
}
