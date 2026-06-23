import type React from "react";
import { FileIcon, FolderIcon } from "lucide-react";
import { formatFileSize, formatPercentage } from "@/utils/formatters";
import type { FileNode } from "@/types";

interface TreeMapTooltipProps {
  visible: boolean;
  x: number;
  y: number;
  data: { name: string; size: number; originalNode: FileNode } | null;
  parentSize: number;
}

export const TreeMapTooltip: React.FC<TreeMapTooltipProps> = ({
  visible,
  x,
  y,
  data,
  parentSize,
}) => {
  if (!visible || !data) return null;

  const isDir = data.originalNode.type === "directory";
  const modified = data.originalNode.lastModified;

  return (
    <div
      className="fixed z-[9999] pointer-events-none"
      style={{ left: x + 14, top: y + 14, transform: "translateZ(0)" }}
    >
      <div className="bg-popover text-popover-foreground border border-border rounded-lg shadow-lg px-3 py-2 max-w-xs">
        <div className="flex items-center space-x-2">
          {isDir ? (
            <FolderIcon className="w-4 h-4 text-muted-foreground shrink-0" />
          ) : (
            <FileIcon className="w-4 h-4 text-muted-foreground shrink-0" />
          )}
          <span className="font-medium text-sm truncate">{data.name}</span>
        </div>
        <div className="mt-1 text-xs text-muted-foreground space-y-0.5">
          <div>
            {formatFileSize(data.size)} · {formatPercentage(data.size, parentSize)} of parent
          </div>
          {modified ? (
            <div>Modified {new Date(modified * 1000).toLocaleDateString()}</div>
          ) : null}
        </div>
      </div>
    </div>
  );
};
