# Theming & color system

## Theming

**Catppuccin** palette: 4 flavors (Latte/light, Frappé/Macchiato/Mocha/dark), 14 accent colors.

- Flavor applied by toggling a `theme-<flavor>` class on `<html>` — no Tailwind `dark:` classes anywhere
- All semantic tokens (`--background`, `--foreground`, `--primary`, etc.) are CSS vars defined per `.theme-*` block in `src/index.css`
- Tailwind v4 `@theme inline` block maps `--color-*` utilities to those vars
- Accent overrides `--primary`, `--primary-foreground`, `--ring`, `--viz-ramp-5` inline on `html` via `applyAccentOverride()` in `useThemeSettings.ts`
- `--primary-foreground` is computed via WCAG luminance (`readableInk()` in `src/lib/colorScale.ts`) — **not a hardcoded table**

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

Both are Radix Popover components:
- `AccentPicker`: trigger = current-color dot + name, opens 14-swatch grid, `side="top"`
- `ThemePicker`: trigger = current flavor name + chevron, opens 5-item list with checkmark, `side="top"`
