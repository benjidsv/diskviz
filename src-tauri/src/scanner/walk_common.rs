//! Platform-independent scaffolding shared between the macOS and Windows
//! native parallel walkers.
//!
//! Both `walk_macos` and `walk_windows` import everything from here so the
//! intermediate-tree types, shared counters, progress throttling logic, and the
//! arena-flattening pass are defined exactly once.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::time::{SystemTime, UNIX_EPOCH};

use super::Node;

// ── Channel message ───────────────────────────────────────────────────────────

pub enum Msg {
    Progress(super::Progress),
    Done(RawNode),
}

// ── Intermediate per-entry tree ───────────────────────────────────────────────

/// Owned tree node produced by the parallel walk. Converted to the arena
/// `Node` layout via `flatten()` after the walk completes.
pub struct RawNode {
    pub name:      String,
    pub size:      u64,   // allocated bytes for files; 0 for dirs
    pub mtime:     i64,   // unix seconds
    pub is_dir:    bool,
    pub is_hidden: bool,
    pub children:  Vec<RawNode>,
}

// ── Shared counters ───────────────────────────────────────────────────────────

pub struct WalkStats {
    pub file_count:    AtomicU64,
    pub dir_count:     AtomicU64,
    pub bytes_scanned: AtomicU64,
    pub last_emit_ms:  AtomicU64,
}

impl Default for WalkStats {
    fn default() -> Self {
        Self {
            file_count:    AtomicU64::new(0),
            dir_count:     AtomicU64::new(0),
            bytes_scanned: AtomicU64::new(0),
            last_emit_ms:  AtomicU64::new(0),
        }
    }
}

// ── Throttled progress emission (~80 ms window) ───────────────────────────────

pub fn emit_progress_if_due(
    current_path: &str,
    stats: &WalkStats,
    denom: u64,
    tx: &Sender<Msg>,
) {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let last = stats.last_emit_ms.load(Ordering::Relaxed);
    if now_ms.saturating_sub(last) < 80 { return; }
    // CAS: only the first winner emits within each 80 ms window.
    if stats.last_emit_ms
        .compare_exchange(last, now_ms, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    let files = stats.file_count.load(Ordering::Relaxed);
    let dirs  = stats.dir_count.load(Ordering::Relaxed);
    let bytes = stats.bytes_scanned.load(Ordering::Relaxed);
    let percent = if denom > 0 {
        ((bytes as f64 / denom as f64) * 100.0).min(99.0)
    } else {
        95.0 * (1.0 - 1.0 / (1.0 + (files + dirs) as f64 / 50_000.0))
    };
    let _ = tx.send(Msg::Progress(super::Progress {
        current_path:        current_path.to_string(),
        files_scanned:       files,
        directories_scanned: dirs,
        bytes_scanned:       bytes,
        percent,
    }));
}

// ── Flatten RawNode tree → arena Vec<Node> ────────────────────────────────────

/// Convert the intermediate `RawNode` tree to the flat `Vec<Node>` arena used
/// by the rest of the scanner. Uses an explicit stack (pre-order DFS) to avoid
/// deep call-stack recursion and to guarantee the **child-idx > parent-idx**
/// invariant that the bottom-up aggregation pass depends on.
pub fn flatten(root_raw: RawNode, root_path: &PathBuf) -> (Vec<Node>, u32) {
    let mut nodes: Vec<Node> = Vec::new();

    // Stack: (raw_node, parent_index).
    let mut stack: Vec<(RawNode, Option<u32>)> = vec![(root_raw, None)];

    while let Some((raw, parent)) = stack.pop() {
        let idx      = nodes.len() as u32;
        let is_hidden = raw.is_hidden;

        // Root node's name is the full scan path (mirrors the jwalk path).
        let name = if parent.is_none() {
            root_path.to_string_lossy().into_owned()
        } else {
            raw.name
        };

        nodes.push(Node {
            name,
            size:       raw.size,
            file_count: 0,
            dir_count:  0,
            is_dir:     raw.is_dir,
            is_hidden,
            mtime:      raw.mtime,
            parent,
            children:   Vec::new(),
        });

        if let Some(p) = parent {
            nodes[p as usize].children.push(idx);
        }

        // Push children in reverse order so that popping (LIFO) yields them
        // left-to-right, giving pre-order DFS. Each child's idx > parent's. ✓
        for child in raw.children.into_iter().rev() {
            stack.push((child, Some(idx)));
        }
    }

    (nodes, 0) // root is always index 0
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a hand-crafted `RawNode` tree and verify that `flatten` produces
    /// an arena where every child index is strictly greater than its parent
    /// index, and that parent links are set correctly.
    #[test]
    fn flatten_child_idx_gt_parent_idx() {
        // Tree layout:
        //   root (dir)
        //     ├── file_a (file, size 100)
        //     └── sub   (dir)
        //           └── file_b (file, size 200)
        let root_raw = RawNode {
            name: "root".into(),
            size: 0,
            mtime: 0,
            is_dir: true,
            is_hidden: false,
            children: vec![
                RawNode {
                    name: "file_a".into(),
                    size: 100,
                    mtime: 1000,
                    is_dir: false,
                    is_hidden: false,
                    children: vec![],
                },
                RawNode {
                    name: "sub".into(),
                    size: 0,
                    mtime: 0,
                    is_dir: true,
                    is_hidden: false,
                    children: vec![RawNode {
                        name: "file_b".into(),
                        size: 200,
                        mtime: 2000,
                        is_dir: false,
                        is_hidden: false,
                        children: vec![],
                    }],
                },
            ],
        };

        let root_path = std::path::PathBuf::from("/base");
        let (nodes, root_idx) = flatten(root_raw, &root_path);

        assert_eq!(root_idx, 0, "root always at index 0");
        assert_eq!(nodes.len(), 4, "root + file_a + sub + file_b");

        // Root node: name replaced with full root_path, no parent.
        assert_eq!(nodes[0].name, "/base");
        assert!(nodes[0].is_dir);
        assert!(nodes[0].parent.is_none());

        // Child-idx > parent-idx invariant for every non-root node.
        for (i, node) in nodes.iter().enumerate() {
            if let Some(p) = node.parent {
                assert!(
                    i as u32 > p,
                    "node {i} has parent {p} but child idx must be > parent idx"
                );
            }
        }

        // Parent links are set correctly.
        for (i, node) in nodes.iter().enumerate().skip(1) {
            let parent_idx = node.parent.expect("non-root must have a parent") as usize;
            assert!(
                nodes[parent_idx].children.contains(&(i as u32)),
                "parent {parent_idx} must list child {i} in its children vec"
            );
        }

        // Verify file sizes were preserved.
        let file_a = nodes.iter().find(|n| n.name == "file_a").unwrap();
        assert_eq!(file_a.size, 100);
        assert_eq!(file_a.mtime, 1000);

        let file_b = nodes.iter().find(|n| n.name == "file_b").unwrap();
        assert_eq!(file_b.size, 200);
        assert_eq!(file_b.mtime, 2000);
    }
}

