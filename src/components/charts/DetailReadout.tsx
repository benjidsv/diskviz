import type React from "react";
import { FileIcon, FolderIcon, PieChartIcon } from "lucide-react";
import { activeness, formatAge, formatFileSize, formatPercentage } from "@/utils/formatters";
import { Dot } from "@/components/ui/dot";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useThemeSettings, VIZ_SUN_COLORS } from "@/hooks/useThemeSettings";
import { compositionSlices, topTypesText, TypeCompositionBar } from "./TypeCompositionBar";
import { TypeCompositionDonut } from "./TypeCompositionDonut";
import type { FileNode } from "@/types";

interface DetailReadoutProps {
  /** The node to describe: a selected child, or the current directory. */
  node: FileNode | null;
  /** Current directory size — denominator for the selection's percentage. */
  parentSize: number;
  /** True when `node` is a selected child; false when it's the current dir. */
  isSelection: boolean;
  /** Staleness threshold (days) for the activeness category label. */
  ageThresholdDays: number;
}

/**
 * Instrument-style readout for the selected (or current) node. Sits in the viz
 * footer strip so the user never has to chase a value with the cursor.
 */
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
  const modified = node.lastModified;
  const slices = compositionSlices(
    node.fileTypes,
    node.fileTypesOther,
    VIZ_SUN_COLORS[resolvedFlavor],
  );
  const avgSize = node.fileCount > 0 ? node.size / node.fileCount : 0;
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
        {avgSize > 0 && (
          <>
            <Dot />
            avg {formatFileSize(avgSize)}
          </>
        )}
        {slices.length > 0 && (
          <>
            <Dot />
            <TypeCompositionBar slices={slices} />
            <span className="truncate">{topTypesText(slices)}</span>
            <Popover>
              <PopoverTrigger
                className="text-muted-foreground hover:text-foreground focus-visible:text-foreground shrink-0"
                aria-label="File-type breakdown"
                title="File-type breakdown"
              >
                <PieChartIcon className="w-3.5 h-3.5" />
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
            median {formatAge(median)} ({act.label})
          </>
        ) : null}
        {modified ? (
          <>
            <Dot />
            {new Date(modified * 1000).toLocaleDateString()}
          </>
        ) : null}
      </span>
    </div>
  );
};
