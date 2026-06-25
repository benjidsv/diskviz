# diskviz

Tauri v2 + React 19 + Rust disk usage visualizer. Scanner uses `jwalk` on the Rust side. Frontend is Vite + Tailwind v4 + shadcn-style Radix UI components.

## Stack

- **Runtime:** Tauri v2 (WKWebView on macOS)
- **Frontend:** React 19, TypeScript, Vite
- **Styling:** Tailwind v4 (`@tailwindcss/vite`), CSS custom properties for theming, no `dark:` classes
- **UI primitives:** Radix UI wrapped in shadcn-style components under `src/components/ui/`
- **Class merging:** `clsx` + `tailwind-merge` via `src/lib/utils.ts`
- **Charts:** `recharts` (treemap), hand-rolled SVG (sunburst, file-type donut)

See [`docs/theming.md`](docs/theming.md) for the color system, Catppuccin palette, and picker details.

## Dev

```bash
npm run dev        # Vite only (port 1420, strictPort)
npm run tauri dev  # full Tauri app
npm run build      # tsc + vite build
npm run test       # vitest (frontend unit tests, run-once)
npm run test:watch # vitest (watch mode)
```

```bash
cd src-tauri
cargo test                                        # Rust unit tests
cargo run --release --example scan -- <path>      # multi-walker comparison harness
cargo run --release --example scan -- <path> --runs 3 --walker custom
```

## Key source files

```
src/
  App.tsx                          # root: scan, selection, error/cancel/empty states, forward/back stack, macOS menu events, drag-drop, swipe gestures, footer (settings)
  index.css                        # theme variable blocks + viz ramp/sun tokens; --radius scale; .readout / .micro-label utilities
  types.ts                         # shared types: FileNode (+ hiddenChildren/hiddenSize, fileTypes/fileTypesOther, medianMtime), FileTypeStat, ScanSummary, ScanProgress
  lib/
    colorScale.ts                  # hexToRgb, interpolateStops, readableInk, relativeLuminance
    api.ts                         # Tauri invoke wrappers (incl. cancelScan; getSubtree takes maxDepth/maxChildren/offset)
    utils.ts                       # cn() helper
  utils/
    formatters.ts                  # formatFileSize, formatDuration, formatNumber, formatPercentage, formatAge, activeness (age→score+label)
  hooks/
    useThemeSettings.ts            # theme/accent state; ACCENT_COLORS, VIZ_RAMP_BASE, VIZ_SUN_COLORS, VIZ_AGE_BASE tables; ageRampStops
    useTreeMapData.ts              # FileNode → treemap data (+ synthetic "Other" tile from hiddenChildren/hiddenSize); OTHER_NODE_ID, isOtherNode
    useTreeMapInteraction.ts       # hover/tooltip state for treemap
    useVisualizationSettings.ts    # persists view (treemap/sunburst), colorMode (size/activeness), ageThresholdDays
    useNativeContextMenu.ts        # native macOS context menus (showNodeContextMenu, showBreadcrumbContextMenu)
  components/
    TopBar.tsx                     # HUD: brand · back/forward · breadcrumb trail · live stats · Open
    ErrorState.tsx                 # full-area scan/navigation error with Retry/Dismiss
    AccentPicker.tsx               # swatch popover (14 color dots)
    ThemePicker.tsx                # flavor picker (Radix Popover)
    ScanProgress.tsx               # scan-in-progress panel (progressbar ARIA + Cancel)
    NoticesModal.tsx               # open-source notices modal
    charts/
      TreeMapChart.tsx             # recharts Treemap + continuous color gradient (size or activeness); single-click select/toggle
      SunburstChart.tsx            # hand-rolled SVG sunburst (size or activeness); single-click select; keyboard a11y
      TreeMapTooltip.tsx           # floating tooltip
      DetailReadout.tsx            # selected/current node readout (viz footer strip): size, %, file/folder counts, composition popover, median-age tag
      ColorScaleLegend.tsx         # color legend; size→color gradient (treemap) or activeness fresh→old ramp
      TypeCompositionBar.tsx       # compact stacked bar of file-type mix; compositionSlices() + topTypesText() helpers
      TypeCompositionDonut.tsx     # donut + legend of file-type mix (composition popover content)
      vizColor.ts                  # sizeColorRgb / activenessColorRgb / neutralRgb — node → fill for both color modes
      DeleteConfirmDialog.tsx      # confirm trash dialog (with item counts)
    ui/
      button.tsx, input.tsx        # shadcn-style primitives
      dot.tsx                      # middot separator for HUD/footer
      toggle-group.tsx             # view switcher (treemap/sunburst)
      alert-dialog.tsx             # Radix AlertDialog wrapper
      popover.tsx                  # Radix Popover wrapper
      separator.tsx
```

## Tests

### Frontend (vitest 3, jsdom)

Configured in `vite.config.ts` (`test.environment: "jsdom"`, `globals: true`).
TypeScript globals come from `"types": ["vitest/globals"]` in `tsconfig.json`.
Test files sit next to the source they cover (`*.test.ts`).

| File | What's tested |
| ---- | ------------- |
| `src/utils/formatters.test.ts` | `formatFileSize`, `formatDuration`, `formatPercentage`, `formatAge`, `activeness` (fake timers) |
| `src/lib/colorScale.test.ts` | `hexToRgb`, `relativeLuminance`, `readableInk`, `interpolateStops`, `rgbToCss` |
| `src/components/charts/vizColor.test.ts` | `sizeColorRgb`, `neutralRgb`, `activenessColorRgb` |
| `src/components/charts/TypeCompositionBar.test.ts` | `compositionSlices`, `topTypesText` |
| `src/hooks/useTreeMapData.test.ts` | `OTHER_NODE_ID`, `isOtherNode` |

### Rust (`cargo test`)

| Module | What's tested |
| ------ | ------------- |
| `scanner/mod.rs` | `aggregates_sizes_and_counts`, `remove_subtree_bucket_consistency`, `macos_walker_parity`, `extension_of` (5 cases), `age_bucket` (4 cases), `remove_subtree` ancestor propagation + saturating-sub guard, `path_of`, `windows_walker_parity` (cfg windows) |
| `commands.rs` | `adaptive_visible_count` (4 cases), `subtree_stats` (5 cases incl. median bucket) via `build_test_tree` helper |
| `scanner/walk_common.rs` | `flatten` child-idx > parent-idx invariant + parent links |
| `scanner/walk_macos.rs` | `walk_basic_tree`, `walk_cancel`, `walk_hardlink_dedup` |
| `scanner/dirmeta.rs` | `dirmeta_parity` |

## Walker API (`scanner/mod.rs`)

```rust
pub enum Walker { Default, Custom, Jwalk }

// Explicit back-end selection — used by tests and the comparison harness.
pub fn scan_with<F: FnMut(Progress)>(
    root: PathBuf, cancel: Arc<AtomicBool>, on_progress: F, walker: Walker,
) -> io::Result<ScanTree>;

// Public API (Tauri commands): delegates to scan_with(.., Walker::Default).
pub fn scan<F: FnMut(Progress)>(
    root: PathBuf, cancel: Arc<AtomicBool>, on_progress: F,
) -> io::Result<ScanTree>;
```

`Walker::Default` selects the platform fast-path (`walk_macos` / `walk_windows`)
unless `DISKVIZ_NO_BULK=1` is set, in which case `jwalk` is used. Tests and
the harness pass `Walker::Custom` or `Walker::Jwalk` directly so no env-var
mutation is needed.

Adding a future `Walker::Mft` variant requires: one enum variant + one
`match` arm in `scan_with` + one entry in the harness's `available_walkers()`.

## Shell

- **TopBar** (`TopBar.tsx`) is the location/stats HUD; it owns the single breadcrumb trail and back/forward buttons. The breadcrumb is the only path display (no duplicate raw path). Root crumb is accented; the `/` volume root suppresses its trailing separator to avoid `//`.
- **Footer** holds settings only (accent, theme, view toggle, color-mode toggle + age-threshold control, shortcuts, notices). Live stats live in the TopBar, not the footer.
- Directory selection is one step: Browse / `⌘O` / drag-drop a folder onto the window all scan immediately (no separate input row or Analyze button).

## Color modes & subtree stats

- **Two color modes** (`useVisualizationSettings`, persisted): `size` colors tiles/arcs by log-normalized size along the accent ramp (`VIZ_RAMP_BASE`); `activeness` colors by median file age (fresh→old, green→red) along `VIZ_AGE_BASE`, scaled by `ageThresholdDays` (default 730). `vizColor.ts` maps a node to a fill in either mode; nodes without age data (synthetic "Other", empty folders) fall back to a neutral tone.
- **Subtree stats are computed in Rust** (`commands.rs` `subtree_stats`) per `get_subtree` call, not in the frontend: top-8 file extensions by size + a summed `fileTypesOther` remainder, and a bucketed-median mtime (`medianMtime`) from a coarse age histogram. Surfaced in `DetailReadout` (composition popover + age tag).
- **Adaptive "Other" bucket** (`commands.rs` `adaptive_visible_count`): shows the smallest N children (min 12, max `maxChildren`) such that the hidden remainder never exceeds the smallest shown tile, so "Other" is never the biggest tile. The "Other" tile stays drillable via `getSubtree(..., offset)` pagination.
- **Scan cancel:** `cancelScan()` flips an `AtomicBool` in `AppState.cancel`, polled inside the scan to abort early and reject the in-flight `scan_directory`.

## Accessibility state

- Treemap tiles: `role="button" tabIndex={0}` + `onKeyDown` (Enter/Space → select). Focus ring via CSS: `g[role="button"]:focus-visible rect { stroke: var(--ring) !important; }` in `index.css`
- Sunburst arcs: `role="button" tabIndex={0}` + `onKeyDown` (Enter/Space → select). Focus ring via CSS: `path[role="button"]:focus-visible { stroke: var(--ring) !important; }` in `index.css`
- Shortcuts modal: `role="dialog" aria-modal="true" aria-labelledby="shortcuts-title"`, autofocuses close button on open, window-level Escape handler
- Scan progress bar: `role="progressbar"` with `aria-valuenow/min/max`

## Known non-issues (intentional)

- `bg-black/50` scrims on modals — intentionally dark regardless of theme
- `* { border-color: var(--border) }` global rule — harmless, pre-existing
- `CustomContent` defined inside `TreeMapChart` (causes remounts on each render) — pre-existing recharts pattern, not worth fixing without broader refactor
- `TreeMapContextMenu.tsx` + `ui/context-menu.tsx` — dead code left in place; right-click is now native macOS via `useNativeContextMenu.ts`
