export interface RGB { r: number; g: number; b: number }

export function hexToRgb(hex: string): RGB {
  const clean = hex.replace("#", "").trim();
  const n = parseInt(clean, 16);
  if (Number.isNaN(n)) return { r: 128, g: 128, b: 128 };
  return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
}

export function rgbToCss(rgb: RGB): string {
  return `rgb(${Math.round(rgb.r)},${Math.round(rgb.g)},${Math.round(rgb.b)})`;
}

function linearize(c: number): number {
  const s = c / 255;
  return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}

export function relativeLuminance(rgb: RGB): number {
  return 0.2126 * linearize(rgb.r) + 0.7152 * linearize(rgb.g) + 0.0722 * linearize(rgb.b);
}

/**
 * Returns near-white (#eff1f5) or near-black (#11111b) with ≥4.5:1 WCAG
 * contrast against the given background.
 */
export function readableInk(bg: RGB): string {
  return relativeLuminance(bg) < 0.179 ? "#eff1f5" : "#11111b";
}

/**
 * Linearly interpolate among N hex color stops at t ∈ [0,1].
 * stops[0] = t=0 (smallest), stops[N-1] = t=1 (largest).
 */
export function interpolateStops(stops: string[], t: number): RGB {
  if (stops.length === 0) return { r: 128, g: 128, b: 128 };
  if (stops.length === 1 || t <= 0) return hexToRgb(stops[0]);
  if (t >= 1) return hexToRgb(stops[stops.length - 1]);

  const segments = stops.length - 1;
  const scaled = t * segments;
  const i = Math.floor(scaled);
  const frac = scaled - i;

  const a = hexToRgb(stops[i]);
  const b = hexToRgb(stops[Math.min(i + 1, stops.length - 1)]);

  return {
    r: a.r + (b.r - a.r) * frac,
    g: a.g + (b.g - a.g) * frac,
    b: a.b + (b.b - a.b) * frac,
  };
}
