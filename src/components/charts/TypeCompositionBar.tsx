import type React from "react";
import { cn } from "@/lib/utils";
import type { FileTypeStat } from "@/types";

export interface CompSlice {
  label: string;
  size: number;
  /** Fraction of the whole, 0..1. */
  pct: number;
  color: string;
}

/** Color used for the aggregated "Other" slice across bar, donut and legend. */
export const OTHER_SLICE_COLOR = "var(--muted-foreground)";

/**
 * Turn a node's top-N extensions (+ the summed remainder) into renderable
 * slices. `colors` is the categorical palette (one hue per named type).
 */
export function compositionSlices(
  fileTypes: FileTypeStat[] | undefined,
  other: number | undefined,
  colors: readonly string[],
): CompSlice[] {
  const types = fileTypes ?? [];
  const otherSize = other ?? 0;
  const total = types.reduce((sum, t) => sum + t.size, 0) + otherSize;
  if (total <= 0) return [];

  const slices: CompSlice[] = types.map((t, i) => ({
    label: t.ext ? t.ext.toUpperCase() : "no ext",
    size: t.size,
    pct: t.size / total,
    color: colors[i % colors.length],
  }));
  if (otherSize > 0) {
    slices.push({
      label: "Other",
      size: otherSize,
      pct: otherSize / total,
      color: OTHER_SLICE_COLOR,
    });
  }
  return slices;
}

/** Inline summary of the top few types, e.g. "PNG 67% · MP4 18%". */
export function topTypesText(slices: CompSlice[], count = 3): string {
  return slices
    .slice(0, count)
    .map((s) => `${s.label} ${Math.round(s.pct * 100)}%`)
    .join(" · ");
}

interface TypeCompositionBarProps {
  slices: CompSlice[];
  className?: string;
}

/** Compact horizontal stacked bar of a folder's file-type composition. */
export const TypeCompositionBar: React.FC<TypeCompositionBarProps> = ({ slices, className }) => {
  if (slices.length === 0) return null;
  return (
    <span
      className={cn(
        "inline-flex h-1.5 w-20 shrink-0 overflow-hidden rounded-full border border-border/60",
        className,
      )}
      aria-hidden
    >
      {slices.map((s, i) => (
        <span
          key={`${s.label}-${i}`}
          style={{ width: `${s.pct * 100}%`, background: s.color }}
          title={`${s.label} ${Math.round(s.pct * 100)}%`}
        />
      ))}
    </span>
  );
};
