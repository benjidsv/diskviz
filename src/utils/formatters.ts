export function formatFileSize(bytes: number): string {
  if (!bytes || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  let size = bytes;
  let i = 0;
  while (size >= 1024 && i < units.length - 1) {
    size /= 1024;
    i++;
  }
  return `${size.toFixed(i === 0 ? 0 : 2)} ${units[i]}`;
}

export function formatNumber(value: number): string {
  return value.toLocaleString();
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

export function formatPercentage(value: number, total: number): string {
  if (!total) return "0.0%";
  return `${((value / total) * 100).toFixed(1)}%`;
}

const SECS_PER_DAY = 86_400;

/** Coarse "how long ago" label for a unix-seconds timestamp (vs now). */
export function formatAge(unixSeconds: number): string {
  if (!unixSeconds) return "—";
  const days = Math.max(0, (Date.now() / 1000 - unixSeconds) / SECS_PER_DAY);
  if (days < 1) return "today";
  if (days < 30) return `${Math.round(days)}d`;
  if (days < 365) return `${Math.round(days / 30)}mo`;
  const years = Math.floor(days / 365);
  const months = Math.round((days % 365) / 30);
  return months > 0 ? `${years}y ${months}mo` : `${years}y`;
}

export type ActivenessLabel = "Active" | "Recent" | "Stale" | "Dormant";

/**
 * Normalize a node's median age to a 0..1 "staleness" score against the
 * configured threshold (in days), plus a coarse category label.
 */
export function activeness(
  unixSeconds: number,
  thresholdDays: number,
): { score: number; label: ActivenessLabel } {
  if (!unixSeconds || thresholdDays <= 0) {
    return { score: 0, label: "Active" };
  }
  const days = Math.max(0, (Date.now() / 1000 - unixSeconds) / SECS_PER_DAY);
  const score = Math.min(1, days / thresholdDays);
  const label: ActivenessLabel =
    score < 0.1 ? "Active" : score < 0.4 ? "Recent" : score < 0.8 ? "Stale" : "Dormant";
  return { score, label };
}
