# diskviz

A fast disk-usage visualizer for macOS. Point it at a folder (or your whole
home directory) and it draws an interactive **TreeMap** and **Sunburst** of
what's actually using your space — then lets you drill in and clear out the
big stuff.

Built with a parallel **Rust + Tauri** backend for speed, with a UI ported
from [vizdisk](https://github.com/kiwamizamurai/vizdisk) (MIT). Themed with
the [Catppuccin](https://github.com/catppuccin/catppuccin) palette (MIT).

> Scans ~3.6M nodes / 259 GB in about **26 seconds**, collecting full size,
> inode, and mtime metadata per entry in parallel.

## Features

- **Two visualizations** — TreeMap and Sunburst, toggle between them live.
- **Drill-down navigation** — double-click any folder to descend; breadcrumbs
  to climb back out.
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

1. **Parallel scan into a flat arena.** `src-tauri/src/scanner.rs` walks the
   tree in parallel with [`jwalk`](https://crates.io/crates/jwalk) (the engine
   behind `dust`), oversubscribing the thread pool ~2× cores to hide per-entry
   `stat()` latency. Results land in a compact index-based `Vec<Node>` (indices,
   not pointers — cache-friendly, and paths are reconstructed on demand rather
   than stored). Directory sizes and file/dir counts are aggregated bottom-up in
   a single reverse pass.

2. **The full tree stays in Rust.** It lives in Tauri managed state. The
   frontend never receives the whole tree — it pulls only the bounded slice it's
   currently rendering via `get_subtree(nodeId, maxDepth, maxChildren)`, so IPC
   payloads stay tiny no matter how many millions of files were scanned.

3. **Streamed progress.** The scan emits throttled `scan-progress` events
   (~every 80 ms / 4k entries) that drive the determinate progress bar.

```
┌──────────────┐   scan_directory(path)   ┌────────────────────────────┐
│   React UI   │ ───────────────────────► │  Rust scanner (jwalk)      │
│  TreeMap /   │ ◄─── scan-progress ───── │  → Vec<Node> in app state  │
│   Sunburst   │   get_subtree(id,d,n)    │                            │
└──────────────┘ ◄───── bounded slice ─── └────────────────────────────┘
```

### Backend command surface

Defined in `src-tauri/src/commands.rs`, called from `src/lib/api.ts`:

| Command | Purpose |
| --- | --- |
| `scan_directory(path)` | Run the parallel walk; stream progress; return totals |
| `get_subtree(nodeId, maxDepth?, maxChildren?)` | Bounded slice of the tree for rendering |
| `get_home_directory()` / `get_common_directories()` | Sensible default scan targets |
| `validate_path(path)` | Check a typed path is a directory |
| `delete_path(path)` | Move to system Trash (reversible) |
| `open_in_finder(path)` | Reveal in Finder |

## Tech stack

- **Backend:** Rust, [Tauri v2](https://v2.tauri.app/), `jwalk`, `dashmap`,
  `trash`, `libc`.
- **Frontend:** React 19, TypeScript, Vite 7, Tailwind CSS v4, shadcn/ui
  (Radix primitives), Recharts, lucide-react.

## Getting started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain)
- Node.js + npm
- macOS (the scanner's allocated-size and firmlink handling are Unix-specific;
  it builds on other platforms but is tuned for and tested on macOS)

### Develop

```bash
npm install
npm run tauri dev
```

### Build a release bundle

```bash
npm run tauri build
```

## Benchmarking the scanner

The scanner has a headless harness, independent of the UI:

```bash
cd src-tauri
cargo run --release --example scan -- /path/to/scan
```

Unit tests cover size/count aggregation:

```bash
cd src-tauri
cargo test
```

## Roadmap

- **`getattrlistbulk` fast path (macOS).** The scan is currently syscall-bound:
  one `lstat()` per entry. macOS's `getattrlistbulk` returns metadata for a whole
  directory in a single syscall, skipping the per-file stat — the path to
  WizTree-class throughput.

## Credits & license

The user interface — TreeMap and Sunburst components, shadcn/ui primitives,
layout and styling — is adapted from **[vizdisk](https://github.com/kiwamizamurai/vizdisk)**
by kiwamizamurai, used under the MIT License. See [NOTICES.md](NOTICES.md).

The Rust scanning backend and the lazy, bounded backend↔UI data flow are
original to diskviz.
