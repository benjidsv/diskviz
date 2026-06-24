import { hexToRgb, interpolateStops, type RGB } from "@/lib/colorScale";

const SECS_PER_DAY = 86_400;

/** Log-normalized size → color across the ramp stops (treemap size mode). */
export function sizeColorRgb(
  size: number,
  minSize: number,
  maxSize: number,
  stops: string[],
): RGB {
  const logMin = Math.log(minSize + 1);
  const logMax = Math.log(maxSize + 1);
  const t = logMax > logMin ? (Math.log(size + 1) - logMin) / (logMax - logMin) : 1;
  return interpolateStops(stops, Math.max(0, Math.min(1, t)));
}

/**
 * Median-age → color across the activeness ramp (fresh → old). Returns null
 * when the node has no age data (synthetic "Other" or empty folders), so the
 * caller can fall back to a neutral fill.
 */
export function activenessColorRgb(
  medianMtime: number | undefined,
  stops: string[],
  thresholdDays: number,
): RGB | null {
  if (!medianMtime || thresholdDays <= 0) return null;
  const ageDays = Math.max(0, (Date.now() / 1000 - medianMtime) / SECS_PER_DAY);
  const t = Math.min(1, ageDays / thresholdDays);
  return interpolateStops(stops, t);
}

/** Neutral tone for nodes without an age in activeness mode. */
export function neutralRgb(sizeStops: string[]): RGB {
  return hexToRgb(sizeStops[0] ?? "#808080");
}
