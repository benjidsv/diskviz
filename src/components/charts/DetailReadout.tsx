import type React from "react";
import { FileIcon, FolderIcon } from "lucide-react";
import { formatFileSize, formatPercentage } from "@/utils/formatters";
import { Dot } from "@/components/ui/dot";
import type { FileNode } from "@/types";

interface DetailReadoutProps {
  /** The node to describe: a selected child, or the current directory. */
  node: FileNode | null;
  /** Current directory size — denominator for the selection's percentage. */
  parentSize: number;
  /** True when `node` is a selected child; false when it's the current dir. */
  isSelection: boolean;
}

/**
 * Instrument-style readout for the selected (or current) node. Sits in the viz
 * footer strip so the user never has to chase a value with the cursor.
 */
export const DetailReadout: React.FC<DetailReadoutProps> = ({
  node,
  parentSize,
  isSelection,
}) => {
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
            {node.fileCount.toLocaleString()} F
            <Dot />
            {node.dirCount.toLocaleString()} D
          </>
        )}
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
