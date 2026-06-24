import type React from "react";
import type { ColorMode } from "@/hooks/useVisualizationSettings";

interface ColorScaleLegendProps {
  /** The active treemap ramp (4 base stops + accent) as hex strings. */
  rampStops: string[];
  /** What the color encodes — size, or file age (activeness). */
  colorMode: ColorMode;
  /** The activeness ramp (fresh → old) as hex strings. */
  ageRampStops: string[];
}

/**
 * Decodes the chart's continuous color mapping. The gradient is built from the
 * same stops `interpolateStops` uses, so the legend matches the tiles/arcs.
 */
export const ColorScaleLegend: React.FC<ColorScaleLegendProps> = ({
  rampStops,
  colorMode,
  ageRampStops,
}) => {
  const isAge = colorMode === "activeness";
  const stops = isAge ? ageRampStops : rampStops;
  if (stops.length === 0) return null;
  const gradient = `linear-gradient(to right, ${stops.join(", ")})`;

  return (
    <div className="flex items-center gap-2 min-w-0">
      <span className="micro-label text-muted-foreground shrink-0">{isAge ? "Age" : "Size"}</span>
      <span className="text-[10px] text-muted-foreground shrink-0">{isAge ? "fresh" : "small"}</span>
      <span
        className="h-1.5 w-24 rounded-full border border-border/60"
        style={{ background: gradient }}
        aria-hidden
      />
      <span className="text-[10px] text-muted-foreground shrink-0">{isAge ? "old" : "large"}</span>
    </div>
  );
};
