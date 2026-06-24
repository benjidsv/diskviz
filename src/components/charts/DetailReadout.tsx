import type React from "react";
import { FileIcon, FolderIcon, PieChartIcon } from "lucide-react";
import { activeness, formatAge, formatFileSize, formatPercentage } from "@/utils/formatters";
import { Dot } from "@/components/ui/dot";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useThemeSettings, VIZ_AGE_BASE, VIZ_SUN_COLORS } from "@/hooks/useThemeSettings";
import { compositionSlices } from "./TypeCompositionBar";
import { TypeCompositionDonut } from "./TypeCompositionDonut";
import type { ActivenessLabel } from "@/utils/formatters";
import type { ThemeFlavor } from "@/hooks/useThemeSettings";
import type { FileNode } from "@/types";

/** Map an activeness label to the matching stop in VIZ_AGE_BASE (green→yellow→peach→red). */
function ageColor(label: ActivenessLabel, flavor: ThemeFlavor): string {
  const stops = VIZ_AGE_BASE[flavor];
  // Active/Recent → green (0), Stale → yellow (1), Dormant → red (3)
  if (label === "Active" || label === "Recent") return stops[0];
  if (label === "Stale") return stops[1];
  return stops[3];
}

interface DetailReadoutProps {
  node: FileNode | null;
  parentSize: number;
  isSelection: boolean;
  ageThresholdDays: number;
}

export const DetailReadout: React.FC<DetailReadoutProps> = ({
  node,
  parentSize,
  isSelection,
  ageThresholdDays,
}) => {
  const { resolvedFlavor } = useThemeSettings();

  if (!node) {
    return (
      <div className="flex items-center gap-2 min-w-0">
        <span className="micro-label text-muted-foreground">Selected</span>
        <span className="text-xs text-muted-foreground truncate">
          Click a tile to inspect
        </span>
      </div>
    );
  }

  const isDir = node.type === "directory";
  const slices = compositionSlices(
    node.fileTypes,
    node.fileTypesOther,
    VIZ_SUN_COLORS[resolvedFlavor],
  );
  const median = node.medianMtime ?? 0;
  const act = median ? activeness(median, ageThresholdDays) : null;

  return (
    <div className="flex items-center gap-2 min-w-0">
      <span className="micro-label text-muted-foreground shrink-0">
        {isSelection ? "Selected" : "Current"}
      </span>
      {isDir ? (
        <FolderIcon className="w-3.5 h-3.5 text-muted-foreground shrink-0" />
      ) : (
        <FileIcon className="w-3.5 h-3.5 text-muted-foreground shrink-0" />
      )}
      <span className="text-xs font-medium text-foreground truncate">{node.name}</span>
      <span className="readout text-xs text-muted-foreground shrink-0 flex items-center gap-2">
        <Dot />
        {formatFileSize(node.size)}
        {isSelection && (
          <>
            <Dot />
            {formatPercentage(node.size, parentSize)}
          </>
        )}
        {(node.fileCount > 0 || node.dirCount > 0) && (
          <>
            <Dot />
            {node.fileCount.toLocaleString()} files
            <Dot />
            {node.dirCount.toLocaleString()} folders
          </>
        )}
        {slices.length > 0 && (
          <>
            <Dot />
            <Popover>
              <PopoverTrigger
                className="flex items-center gap-1.5 text-muted-foreground hover:text-foreground focus-visible:text-foreground shrink-0"
                aria-label="File-type breakdown"
                title="File-type breakdown"
              >
                <PieChartIcon className="w-3 h-3" />
                <span>Composition</span>
              </PopoverTrigger>
              <PopoverContent className="w-auto p-3">
                <TypeCompositionDonut slices={slices} />
              </PopoverContent>
            </Popover>
          </>
        )}
        {act ? (
          <>
            <Dot />
            <span style={{ color: ageColor(act.label, resolvedFlavor) }}>
              median {formatAge(median)} · {act.label}
            </span>
          </>
        ) : null}
      </span>
    </div>
  );
};
