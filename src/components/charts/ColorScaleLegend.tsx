import type React from "react";

interface ColorScaleLegendProps {
  /** The active treemap ramp (4 base stops + accent) as hex strings. */
  rampStops: string[];
}

/**
 * Decodes the treemap's continuous size→color mapping. The gradient is built
 * from the same stops `interpolateStops` uses, so the legend matches the tiles.
 */
export const ColorScaleLegend: React.FC<ColorScaleLegendProps> = ({ rampStops }) => {
  if (rampStops.length === 0) return null;
  const gradient = `linear-gradient(to right, ${rampStops.join(", ")})`;

  return (
    <div className="flex items-center gap-2 min-w-0">
      <span className="micro-label text-muted-foreground shrink-0">Size</span>
      <span className="text-[10px] text-muted-foreground shrink-0">small</span>
      <span
        className="h-1.5 w-24 rounded-full border border-border/60"
        style={{ background: gradient }}
        aria-hidden
      />
      <span className="text-[10px] text-muted-foreground shrink-0">large</span>
    </div>
  );
};
