# diskviz

Tauri v2 + React 19 + Rust disk usage visualizer. Scanner uses `jwalk` on the Rust side. Frontend is Vite + Tailwind v4 + shadcn-style Radix UI components.

## Stack

- **Runtime:** Tauri v2 (WKWebView on macOS)
- **Frontend:** React 19, TypeScript, Vite
- **Styling:** Tailwind v4 (`@tailwindcss/vite`), CSS custom properties for theming, no `dark:` classes
- **UI primitives:** Radix UI wrapped in shadcn-style components under `src/components/ui/`
- **Class merging:** `clsx` + `tailwind-merge` via `src/lib/utils.ts`
- **Charts:** `recharts` (treemap), hand-rolled SVG (sunburst)

## Dev

```bash
npm run dev        # Vite only (port 1420, strictPort)
npm run tauri dev  # full Tauri app
npm run build      # tsc + vite build
```

## Theming

**Catppuccin** palette: 4 flavors (Latte/light, Frappé/Macchiato/Mocha/dark), 14 accent colors.

- Flavor applied by toggling a `theme-<flavor>` class on `<html>` — no Tailwind `dark:` classes anywhere
- All semantic tokens (`--background`, `--foreground`, `--primary`, etc.) are CSS vars defined per `.theme-*` block in `src/index.css`
- Tailwind v4 `@theme inline` block maps `--color-*` utilities to those vars
- Accent overrides `--primary`, `--primary-foreground`, `--ring`, `--viz-ramp-5` inline on `html` via `applyAccentOverride()` in `useThemeSettings.ts`
- `--primary-foreground` is computed via WCAG luminance (`readableInk()` in `src/lib/colorScale.ts`) — **not a hardcoded table**

## Key source files

```
src/
  App.tsx                          # root: path input, scan, breadcrumbs, footer controls
  index.css                        # all theme variable blocks + viz ramp/sun tokens
  lib/
    colorScale.ts                  # hexToRgb, interpolateStops, readableInk, relativeLuminance
    api.ts                         # Tauri invoke wrappers
    utils.ts                       # cn() helper
  hooks/
    useThemeSettings.ts            # theme/accent state; ACCENT_COLORS, VIZ_RAMP_BASE, VIZ_SUN_COLORS tables
    useTreeMapData.ts              # transforms FileNode → treemap data + maxSize/minSize/totalSize
    useTreeMapInteraction.ts       # hover/tooltip/context-menu state for treemap
    useVisualizationSettings.ts    # persists treemap vs sunburst choice
  components/
    AccentPicker.tsx               # swatch popover (14 color dots)
    ThemePicker.tsx                # flavor picker (Radix Popover)
    ScanProgress.tsx               # scan-in-progress panel
    charts/
      TreeMapChart.tsx             # recharts Treemap + continuous color gradient
      SunburstChart.tsx            # hand-rolled SVG sunburst
      TreeMapTooltip.tsx           # floating tooltip
      TreeMapContextMenu.tsx       # right-click menu
      DeleteConfirmDialog.tsx      # confirm trash dialog
    ui/
      button.tsx, input.tsx        # shadcn-style primitives
      toggle-group.tsx             # view switcher (treemap/sunburst)
      context-menu.tsx             # Radix ContextMenu wrapper
      alert-dialog.tsx             # Radix AlertDialog wrapper
      popover.tsx                  # Radix Popover wrapper
      separator.tsx
```

## Treemap coloring

Continuous perceptual heatmap — **no buckets**. Each cell gets a unique shade.

- `VIZ_RAMP_BASE[flavor]` in `useThemeSettings.ts`: 4 static hex stops per flavor (Surface0 → Surface2 → Lavender → Sapphire)
- 5th stop = current accent color (`accentColor` from `useThemeSettings`)
- App.tsx computes `rampStops = [...VIZ_RAMP_BASE[resolvedFlavor], accentColor]` and passes to `<TreeMapChart>`
- In `TreeMapChart`: `t = (log(size+1) - log(min+1)) / (log(max+1) - log(min+1))` → `interpolateStops(rampStops, t)` → RGB fill
- Label color computed with `readableInk(cellRgb)` — adapts to light/dark tiles automatically
- CSS vars `--viz-ramp-1..5` in `index.css` match the JS table (used as fallback / for any direct CSS reference)

## Sunburst coloring

- Arc fills: `var(--viz-sun-0..6)` cycling by depth level — static per flavor, NOT affected by accent
- `VIZ_SUN_COLORS[flavor]` in `useThemeSettings.ts`: 7 hex stops per flavor (matches `index.css`)
- Arc label ink: `readableInk(hexToRgb(VIZ_SUN_COLORS[resolvedFlavor][level % 7]))` — computed in `SunburstChart` via `useThemeSettings()`

## Pickers (footer)

Both replaced from native `<select>` to Radix Popover components:
- `AccentPicker`: trigger = current-color dot + name, opens 14-swatch grid, `side="top"`
- `ThemePicker`: trigger = current flavor name + chevron, opens 5-item list with checkmark, `side="top"`

## Accessibility state

- Treemap tiles: `role="button" tabIndex={0}` + `onKeyDown` (Enter/Space → click). Focus ring via CSS: `g[role="button"]:focus-visible rect { stroke: var(--ring) !important; }` in `index.css`
- Shortcuts modal: `role="dialog" aria-modal="true" aria-labelledby="shortcuts-title"`, autofocuses close button on open, window-level Escape handler
- Sunburst paths already had `role="button"` + keyboard handler

## Known non-issues (intentional)

- `bg-black/50` scrims on modals — intentionally dark regardless of theme
- `* { border-color: var(--border) }` global rule — harmless, pre-existing
- `CustomContent` defined inside `TreeMapChart` (causes remounts on each render) — pre-existing recharts pattern, not worth fixing without broader refactor
