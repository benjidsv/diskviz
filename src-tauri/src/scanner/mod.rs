//! Fast parallel filesystem scanner.
//!
//! Walks a directory tree in parallel (via `jwalk`) and stores the result in a
//! compact index-based arena (`Vec<Node>`). Directory sizes are aggregated
//! bottom-up in a single reverse pass. Subtree extension/age stats are also
//! precomputed bottom-up (Phase A) so navigation lookups are O(1). The
//! child-sort pass is parallelised via rayon (Phase B). On macOS, per-entry
//! lstat is replaced by one `getattrlistbulk` syscall per directory (Phase C).

pub mod dirmeta;

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
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let age_days = (now - node.mtime).max(0) / SECS_PER_DAY;
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

// ── scan() ────────────────────────────────────────────────────────────────────

pub fn scan<F: FnMut(Progress)>(
    root: PathBuf,
    cancel: Arc<AtomicBool>,
    mut on_progress: F,
) -> std::io::Result<ScanTree> {
    let start = Instant::now();
    let denom = volume_used_bytes(&root).unwrap_or(0);

    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8)
        .saturating_mul(2)
        .max(4);

    #[cfg(unix)]
    let visited: dashmap::DashSet<(u64, u64)> = dashmap::DashSet::new();

    let use_bulk = std::env::var_os("DISKVIZ_NO_BULK").is_none();

    let cancel_walk = cancel.clone();
    let walk = WalkDirGeneric::<((), EntryMeta)>::new(&root)
        .skip_hidden(false)
        .follow_links(false)
        .parallelism(jwalk::Parallelism::RayonNewPool(threads))
        .process_read_dir(move |_depth, path, _read_dir_state, children| {
            if cancel_walk.load(Ordering::Relaxed) {
                children.clear();
                return;
            }

            // Phase C: one bulk syscall per directory on macOS.
            let bulk: Option<std::collections::HashMap<std::ffi::OsString, dirmeta::RawMeta>> =
                if use_bulk { dirmeta::bulk_dir_meta(path) } else { None };

            for entry in children.iter_mut().flatten() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;

                    if let Some(ref b) = bulk {
                        if let Some(rm) = b.get(entry.file_name()) {
                            let is_dir = rm.is_dir;
                            let mtime  = rm.mtime;
                            let size   = rm.size;

                            if is_dir || rm.nlink > 1 {
                                if !visited.insert((rm.dev, rm.ino)) {
                                    entry.client_state = EntryMeta { size: 0, mtime };
                                    if is_dir { entry.read_children_path = None; }
                                    continue;
                                }
                            }
                            entry.client_state = EntryMeta { size, mtime };
                            continue;
                        }
                    }

                    // Per-entry fallback (non-macOS, DISKVIZ_NO_BULK, or lookup miss).
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
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    struct DirAccum {
        exts: HashMap<u32, u64>,
        age_hist: [u64; 9],
    }

    let mut ext_map: HashMap<String, u32> = HashMap::new();
    let mut ext_names: Vec<String> = Vec::new();
    // Only dir nodes get Some; files stay None.
    let mut accum: Vec<Option<DirAccum>> = nodes
        .iter()
        .map(|n| if n.is_dir { Some(DirAccum { exts: HashMap::new(), age_hist: [0; 9] }) } else { None })
        .collect();

    for i in (0..nodes.len()).rev() {
        let p_opt = nodes[i].parent;
        if !nodes[i].is_dir {
            // File: fold ext + age into parent directory's accumulator.
            let ext_str = extension_of(&nodes[i].name);
            let n_ext = ext_names.len() as u32;
            let ext_id = *ext_map.entry(ext_str.clone()).or_insert_with(|| {
                ext_names.push(ext_str);
                n_ext
            });
            let age_days = (now_secs - nodes[i].mtime).max(0) / SECS_PER_DAY;
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

    // Finalize: convert accumulators to sorted DirStats.
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

    Ok(ScanTree {
        nodes,
        root: root_index,
        root_path: root,
        total_size,
        total_files,
        total_dirs,
        scan_duration_ms: start.elapsed().as_millis() as u64,
        ext_names,
        dir_stats,
    })
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
}
