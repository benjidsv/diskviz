# diskviz

A fast disk-usage visualizer for macOS and Windows. Point it at a folder (or
your whole home directory) and it draws an interactive **TreeMap** and
**Sunburst** of what's actually using your space — then lets you drill in and
clear out the big stuff.

Built with a parallel **Rust + Tauri** backend for speed, with a UI ported
from [vizdisk](https://github.com/kiwamizamurai/vizdisk) (MIT). Themed with
the [Catppuccin](https://github.com/catppuccin/catppuccin) palette (MIT).

> Scans ~3.6M nodes / 259 GB in about **26 seconds**, collecting full size,
> inode, and mtime metadata per entry in parallel.

## Screenshots

### Treemap visual
<img width="1312" height="932" alt="Screenshot 2026-06-24 at 03 17 03" src="https://github.com/user-attachments/assets/e5ddafa1-d49c-4f79-9e1e-7b5cb2204040" />

### Sunburst visual with staleness coloring
<img width="1312" height="932" alt="Screenshot 2026-06-24 at 03 18 30" src="https://github.com/user-attachments/assets/dee92297-5588-47b3-a852-7b65bc7dd850" />

### Folder composition
<img width="354" height="226" alt="Screenshot 2026-06-24 at 03 17 48" src="https://github.com/user-attachments/assets/e22543be-9683-4200-85bc-bab2fd3da0c2" />

## Features

- **Two visualizations** — TreeMap and Sunburst, toggle between them live.
- **Color by size or age** — switch the color mode between *size* (big tiles
  glow) and *activeness*, which tints each folder by the median age of its
  files (fresh → green, dormant → red) against a configurable threshold, so
  stale space jumps out.
- **File-type composition** — every folder shows its mix of file types as a
  click-to-expand donut (e.g. `PNG 67% · MP4 18% · …`), with the long tail
  summed into an honest "Other" slice.
- **Drill-down navigation** — double-click any folder to descend; breadcrumbs
  to climb back out. Crowded folders collapse their long tail into an "Other"
  tile that's adaptively sized (never the biggest tile) and still drillable.
- **Real progress bar** — a genuine determinate bar (not a spinner), with a
  denominator derived from actual volume usage via `statvfs`.
- **Accurate sizes** — reports *allocated* size (disk blocks), matching `du`,
  WizTree and DaisyDisk. Sparse files, APFS clones and transparent compression
  are measured correctly, so VM and Docker disk images don't overcount.
- **`du`-correct totals** — `(device, inode)` dedup means hardlinks and macOS
  firmlinks (e.g. `/Users` vs `/System/Volumes/Data/Users`) are counted once,
  so a whole-disk scan isn't inflated.
- **Safe delete** — remove a file or folder straight from the chart; it goes to
  the system Trash (reversible). Sizes update instantly without a rescan.
- **Four themes** — Catppuccin Latte, Frappé, Macchiato, and Mocha. Defaults to
  your OS appearance; change any time via the Theme selector.
- **Reveal in Finder** and a keyboard-shortcut overlay (`⌘O`, `⌘?`).

## Architecture

The speed comes from three deliberate choices working together:

1. **Platform-native bulk enumeration.** On macOS the scanner uses
   `getattrlistbulk` as the sole enumeration primitive — each directory is
   read once and returns name, allocated size, mtime, and inode in a single
   syscall. On Windows, `GetFileInformationByHandleEx` (`FileIdBothDirectoryInfo`)
   does the same. Both replace `jwalk` on their platform. A `Walker` enum
   (`Custom` | `Jwalk` | `Default`) lets tests and the comparison harness pick
   a specific back-end without env-var juggling; `scan()` always uses
   `Walker::Default` (platform fast-path, env-var fallback).

2. **Parallel walk into a flat arena.** Results land in a compact index-based
   `Vec<Node>` (indices, not pointers — cache-friendly). Subdirectories are
   recursed in parallel via `rayon`. Directory sizes, file/dir counts, per-dir
   extension histograms, and age histograms are aggregated bottom-up in a
   single reverse pass so navigation lookups are O(1).

3. **The full tree stays in Rust.** It lives in Tauri managed state. The
   frontend never receives the whole tree — it pulls only the bounded slice it's
   currently rendering via `get_subtree(nodeId, maxDepth, maxChildren, offset)`,
   which also rolls up that subtree's file-type composition and median file age
   on demand, so IPC payloads stay tiny no matter how many millions of files
   were scanned.

4. **Streamed progress.** The scan emits throttled `scan-progress` events
   (~every 80 ms / 4k entries) that drive the determinate progress bar.

```
┌──────────────┐   scan_directory(path)   ┌────────────────────────────────┐
│   React UI   │ ───────────────────────► │  Rust scanner                  │
│  TreeMap /   │ ◄─── scan-progress ───── │  macOS: getattrlistbulk        │
│   Sunburst   │   get_subtree(id,d,n)    │  Windows: FileIdBothDirInfo    │
└──────────────┘ ◄───── bounded slice ─── │  fallback: jwalk               │
                                          │  → Vec<Node> in app state      │
                                          └────────────────────────────────┘
```

### Backend command surface

Defined in `src-tauri/src/commands.rs`, called from `src/lib/api.ts`:

| Command | Purpose |
| --- | --- |
| `scan_directory(path)` | Run the parallel walk; stream progress; return totals |
| `cancel_scan()` | Abort the in-flight scan (polled mid-walk) |
| `get_subtree(nodeId, maxDepth?, maxChildren?, offset?)` | Bounded slice of the tree for rendering, with per-node file-type and median-age stats; `offset` paginates the "Other" bucket |
| `get_home_directory()` / `get_common_directories()` | Sensible default scan targets |
| `validate_path(path)` | Check a typed path is a directory |
| `delete_path(path)` | Move a path to system Trash (reversible) |
| `delete_node(nodeId)` | Trash a node and update in-memory totals without a rescan |
| `open_in_finder(path)` | Reveal in Finder |

## Tech stack

- **Backend:** Rust, [Tauri v2](https://v2.tauri.app/), `rayon`, `jwalk`,
  `dashmap`, `trash`, `libc`, `windows-sys`.
- **Frontend:** React 19, TypeScript, Vite 7, Tailwind CSS v4, shadcn/ui
  (Radix primitives), Recharts, lucide-react.
- **Testing:** [vitest](https://vitest.dev/) (frontend, 79 tests), `cargo test`
  (Rust, 29 tests).

## Getting started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain)
- Node.js + npm
- macOS (primary target; also builds on Windows with the native fast-path
  walker, but UI polish and testing are macOS-first)

### Develop

```bash
npm install
npm run tauri dev
```

### Build a release bundle

```bash
npm run tauri icon icons/diskviz-icon.png # You can use icons/diskviz-icon-shadow.png for macos
npm run tauri build
```

## Testing

### Frontend tests (vitest)

```bash
npm run test          # run once
npm run test:watch    # watch mode
```

79 tests across five files: `formatters`, `colorScale`, `vizColor`,
`TypeCompositionBar`, and `useTreeMapData`.

### Rust tests

```bash
cd src-tauri
cargo test
```

29 tests covering size/count aggregation, `extension_of`, `age_bucket`,
`remove_subtree` ancestor propagation, `path_of`, `adaptive_visible_count`,
`subtree_stats` median bucketing, the `flatten` arena invariant, and
walker parity (macOS custom ↔ jwalk; Windows equivalent compiles but runs
only on Windows).

## Scanner comparison harness

Runs every walker available on the current platform on the same path and
prints a side-by-side results table with a speedup column:

```bash
cd src-tauri
cargo run --release --example scan -- /path/to/scan
```

Flags:

| Flag | Default | Description |
| --- | --- | --- |
| `--runs N` | 1 | Average timing over N runs |
| `--walker <name>` | *(all)* | Run only `custom` or `jwalk` |

The harness flags any divergence in files / dirs / size across walkers and
exits with code 1 if walkers disagree. Adding a future walker is a one-line
entry in the `available_walkers()` list.

## Roadmap

- **MFT/WizTree-style Windows walker.** A third `Walker::Mft` variant that
  reads the NTFS Master File Table directly (like WizTree) for whole-disk
  scans. The `Walker` enum and `scan_with` seam are already in place;
  adding it requires one new enum variant and one `match` arm.

## Credits & license

The user interface — TreeMap and Sunburst components, shadcn/ui primitives,
layout and styling — is adapted from **[vizdisk](https://github.com/kiwamizamurai/vizdisk)**
by kiwamizamurai, used under the MIT License. See [NOTICES.md](NOTICES.md).

The Rust scanning backend and the lazy, bounded backend↔UI data flow are
original to diskviz.
