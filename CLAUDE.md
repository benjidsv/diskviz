# diskviz

Tauri v2 + React 19 + Rust disk usage visualizer. Scanner uses `jwalk` on the Rust side. Frontend is Vite + Tailwind v4 + shadcn-style Radix UI components.

## Stack

- **Runtime:** Tauri v2 (WKWebView on macOS)
- **Frontend:** React 19, TypeScript, Vite
- **Styling:** Tailwind v4 (`@tailwindcss/vite`), CSS custom properties for theming, no `dark:` classes
- **UI primitives:** Radix UI wrapped in shadcn-style components under `src/components/ui/`
- **Class merging:** `clsx` + `tailwind-merge` via `src/lib/utils.ts`
- **Charts:** `recharts` (treemap), hand-rolled SVG (sunburst)

See [`docs/theming.md`](docs/theming.md) for the color system, Catppuccin palette, and picker details.

## Dev

```bash
npm run dev        # Vite only (port 1420, strictPort)
npm run tauri dev  # full Tauri app
npm run build      # tsc + vite build
```

## Key source files

```
src/
  App.tsx                          # root: scan, selection, error/cancel/empty states, forward/back stack, macOS menu events, drag-drop, swipe gestures, footer (settings)
  index.css                        # theme variable blocks + viz ramp/sun tokens; --radius scale; .readout / .micro-label utilities
  types.ts                         # shared types: FileNode (+ hiddenChildren/hiddenSize), ScanSummary, ScanProgress
  lib/
    colorScale.ts                  # hexToRgb, interpolateStops, readableInk, relativeLuminance
    api.ts                         # Tauri invoke wrappers (incl. cancelScan)
    utils.ts                       # cn() helper
  utils/
    formatters.ts                  # formatFileSize, formatDuration, formatNumber, formatPercentage
  hooks/
    useThemeSettings.ts            # theme/accent state; ACCENT_COLORS, VIZ_RAMP_BASE, VIZ_SUN_COLORS tables
    useTreeMapData.ts              # FileNode → treemap data (+ synthetic "Other" tile from hiddenChildren/hiddenSize); OTHER_NODE_ID, isOtherNode
    useTreeMapInteraction.ts       # hover/tooltip state for treemap
    useVisualizationSettings.ts    # persists treemap vs sunburst choice
    useNativeContextMenu.ts        # native macOS context menus (showNodeContextMenu, showBreadcrumbContextMenu)
  components/
    TopBar.tsx                     # HUD: brand · back/forward · breadcrumb trail · live stats · Open
    ErrorState.tsx                 # full-area scan/navigation error with Retry/Dismiss
    AccentPicker.tsx               # swatch popover (14 color dots)
    ThemePicker.tsx                # flavor picker (Radix Popover)
    ScanProgress.tsx               # scan-in-progress panel (progressbar ARIA + Cancel)
    NoticesModal.tsx               # open-source notices modal
    charts/
      TreeMapChart.tsx             # recharts Treemap + continuous color gradient; single-click select
      SunburstChart.tsx            # hand-rolled SVG sunburst; single-click select; keyboard a11y
      TreeMapTooltip.tsx           # floating tooltip
      DetailReadout.tsx            # selected/current node readout (viz footer strip)
      ColorScaleLegend.tsx         # treemap size→color gradient legend
      DeleteConfirmDialog.tsx      # confirm trash dialog (with item counts)
    ui/
      button.tsx, input.tsx        # shadcn-style primitives
      dot.tsx                      # middot separator for HUD/footer
      toggle-group.tsx             # view switcher (treemap/sunburst)
      alert-dialog.tsx             # Radix AlertDialog wrapper
      popover.tsx                  # Radix Popover wrapper
      separator.tsx
```

## Shell

- **TopBar** (`TopBar.tsx`) is the location/stats HUD; it owns the single breadcrumb trail and back/forward buttons. The breadcrumb is the only path display (no duplicate raw path). Root crumb is accented; the `/` volume root suppresses its trailing separator to avoid `//`.
- **Footer** holds settings only (accent, theme, view toggle, shortcuts, notices). Live stats live in the TopBar, not the footer.
- Directory selection is one step: Browse / `⌘O` / drag-drop a folder onto the window all scan immediately (no separate input row or Analyze button).

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
