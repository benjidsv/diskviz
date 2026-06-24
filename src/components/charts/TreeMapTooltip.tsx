import type React from "react";
import { useLayoutEffect, useRef, useState } from "react";
import { FileIcon, FolderIcon } from "lucide-react";
import { formatAge, formatFileSize, formatPercentage } from "@/utils/formatters";
import { useThemeSettings, VIZ_SUN_COLORS } from "@/hooks/useThemeSettings";
import { compositionSlices, topTypesText, TypeCompositionBar } from "./TypeCompositionBar";
import type { FileNode } from "@/types";

interface TreeMapTooltipProps {
  visible: boolean;
  x: number;
  y: number;
  data: { name: string; size: number; originalNode: FileNode } | null;
  parentSize: number;
}

const OFFSET_X = 10;
const OFFSET_Y = 14;
const EDGE_PADDING = 10;

export const TreeMapTooltip: React.FC<TreeMapTooltipProps> = ({
  visible,
  x,
  y,
  data,
  parentSize,
}) => {
  const innerRef = useRef<HTMLDivElement>(null);
  const [dims, setDims] = useState({ w: 0, h: 0 });
  const { resolvedFlavor } = useThemeSettings();

  // Re-measure whenever content changes (new file hovered)
  useLayoutEffect(() => {
    if (!innerRef.current) return;
    const { offsetWidth: w, offsetHeight: h } = innerRef.current;
    setDims((prev) => (prev.w === w && prev.h === h ? prev : { w, h }));
  }, [data]);

  if (!visible || !data) return null;

  const isDir = data.originalNode.type === "directory";
  const median = data.originalNode.medianMtime ?? 0;
  const slices = compositionSlices(
    data.originalNode.fileTypes,
    data.originalNode.fileTypesOther,
    VIZ_SUN_COLORS[resolvedFlavor],
  );

  const flipX = dims.w > 0 && x + OFFSET_X + dims.w + EDGE_PADDING > window.innerWidth;
  const flipY = dims.h > 0 && y + OFFSET_Y + dims.h + EDGE_PADDING > window.innerHeight;

  return (
    <div
      className="fixed z-[9999] pointer-events-none w-max"
      style={{
        left: flipX ? x - OFFSET_X : x + OFFSET_X,
        top: flipY ? y - OFFSET_Y : y + OFFSET_Y,
        transform: `translate(${flipX ? "-100%" : "0"}, ${flipY ? "-100%" : "0"}) translateZ(0)`,
      }}
    >
      <div ref={innerRef} className="bg-popover text-popover-foreground border border-border rounded-lg shadow-lg px-3 py-2">
        <div className="flex items-center space-x-2">
          {isDir ? (
            <FolderIcon className="w-4 h-4 text-muted-foreground shrink-0" />
          ) : (
            <FileIcon className="w-4 h-4 text-muted-foreground shrink-0" />
          )}
          <span className="font-medium text-sm truncate">{data.name}</span>
        </div>
        <div className="mt-1 text-xs text-muted-foreground space-y-0.5 font-mono tabular-nums">
          <div>
            {formatFileSize(data.size)} · {formatPercentage(data.size, parentSize)} of parent
          </div>
          {slices.length > 0 ? (
            <div className="space-y-0.5">
              <TypeCompositionBar slices={slices} className="w-full" />
              <div>{topTypesText(slices, 2)}</div>
            </div>
          ) : null}
          {median ? <div>Median age {formatAge(median)}</div> : null}
        </div>
      </div>
    </div>
  );
};
