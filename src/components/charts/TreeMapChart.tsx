import type React from "react";
import { useCallback, useState } from "react";
import { ResponsiveContainer, Treemap } from "recharts";
import { useTreeMapData } from "@/hooks/useTreeMapData";
import { useTreeMapInteraction } from "@/hooks/useTreeMapInteraction";
import { formatFileSize } from "@/utils/formatters";
import { openInFinder } from "@/lib/api";
import type { FileNode } from "@/types";
import { DeleteConfirmDialog } from "./DeleteConfirmDialog";
import { TreeMapContextMenu } from "./TreeMapContextMenu";
import { TreeMapTooltip } from "./TreeMapTooltip";

interface TreeMapChartProps {
  data: FileNode;
  onNodeClick?: (node: FileNode) => void;
  onNodeDoubleClick?: (node: FileNode) => void;
  onNodeDeleted?: (node: FileNode) => void;
}

const TreeMapChart: React.FC<TreeMapChartProps> = ({
  data,
  onNodeClick,
  onNodeDoubleClick,
  onNodeDeleted,
}) => {
  const { data: treeMapData, maxSize, totalSize } = useTreeMapData(data);
  const { tooltip, contextMenuNode, handleMouseEnter, handleMouseLeave, handleContextMenu } =
    useTreeMapInteraction();

  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [selectedNode, setSelectedNode] = useState<FileNode | null>(null);

  const getColor = useCallback((size: number, max: number): string => {
    const ratio = size / max;
    if (ratio > 0.7) return "var(--viz-ramp-5)";
    if (ratio > 0.5) return "var(--viz-ramp-4)";
    if (ratio > 0.3) return "var(--viz-ramp-3)";
    if (ratio > 0.15) return "var(--viz-ramp-2)";
    return "var(--viz-ramp-1)";
  }, []);

  const handleOpenInFinder = useCallback(async (node: FileNode) => {
    try {
      await openInFinder(node.path);
    } catch (error) {
      console.error("Failed to open in finder:", error);
    }
  }, []);

  const handleDeleteClick = useCallback((node: FileNode) => {
    setSelectedNode(node);
    setDeleteDialogOpen(true);
  }, []);

  const handleDeleteConfirm = useCallback(() => {
    if (!selectedNode) return;
    setDeleteDialogOpen(false);
    onNodeDeleted?.(selectedNode);
    setSelectedNode(null);
  }, [selectedNode, onNodeDeleted]);

  const handleDeleteCancel = useCallback(() => {
    setDeleteDialogOpen(false);
    setSelectedNode(null);
  }, []);

  interface CustomContentProps {
    x: number;
    y: number;
    width: number;
    height: number;
    name: string;
    size: number;
    originalNode: FileNode;
  }

  const CustomContent: React.FC<CustomContentProps> = ({
    x,
    y,
    width,
    height,
    name,
    size,
    originalNode,
  }) => {
    if (!originalNode || typeof width !== "number" || typeof height !== "number") {
      return null;
    }

    const gap = 2;
    const adjustedX = x + gap / 2;
    const adjustedY = y + gap / 2;
    const adjustedWidth = width - gap;
    const adjustedHeight = height - gap;

    if (adjustedWidth <= 0 || adjustedHeight <= 0) {
      return null;
    }

    const isSmall = adjustedWidth < 40 || adjustedHeight < 20;
    const fontSize = Math.max(
      isSmall ? 8 : 10,
      Math.min(adjustedWidth / 6, adjustedHeight / 3, 16),
    );
    const shouldShowText = adjustedWidth > 30 && adjustedHeight > 20;

    const handleClick = (e: React.MouseEvent) => {
      e.preventDefault();
      onNodeClick?.(originalNode);
    };

    const handleDoubleClick = (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      onNodeDoubleClick?.(originalNode);
    };

    const handleContextMenuEvent = (_e: React.MouseEvent) => {
      handleContextMenu(originalNode);
    };

    const handleMouseEnterEvent = (e: React.MouseEvent) => {
      handleMouseEnter({ name, size, originalNode }, e);
    };

    return (
      <g
        role="button"
        tabIndex={0}
        style={{ cursor: "pointer" }}
        onClick={handleClick}
        onDoubleClick={handleDoubleClick}
        onContextMenu={handleContextMenuEvent}
        onMouseEnter={handleMouseEnterEvent}
        onMouseLeave={handleMouseLeave}
      >
        <rect
          x={adjustedX}
          y={adjustedY}
          width={adjustedWidth}
          height={adjustedHeight}
          fill={getColor(size, maxSize)}
          stroke="var(--viz-stroke)"
          strokeWidth={isSmall ? 0.5 : 1}
          rx={4}
          ry={4}
        />

        {shouldShowText && (
          <>
            <text
              x={adjustedX + adjustedWidth / 2}
              y={adjustedY + adjustedHeight / 2 - (isSmall ? 0 : fontSize / 4)}
              textAnchor="middle"
              fill="#ffffff"
              fontSize={fontSize}
              fontWeight="600"
              style={{
                textShadow: "2px 2px 4px rgba(0,0,0,0.9)",
                fontFamily: "system-ui, -apple-system, sans-serif",
              }}
            >
              {name.length > Math.floor(adjustedWidth / (fontSize * 0.6))
                ? `${name.substring(0, Math.floor(adjustedWidth / (fontSize * 0.6)) - 3)}...`
                : name}
            </text>

            {adjustedHeight > 60 && !isSmall && (
              <text
                x={adjustedX + adjustedWidth / 2}
                y={adjustedY + adjustedHeight / 2 + fontSize / 2 + 4}
                textAnchor="middle"
                fill="#ffffff"
                fontSize={fontSize * 0.75}
                fontWeight="500"
                style={{
                  textShadow: "2px 2px 4px rgba(0,0,0,0.9)",
                  fontFamily: "system-ui, -apple-system, sans-serif",
                }}
              >
                {formatFileSize(size)}
              </text>
            )}
          </>
        )}
      </g>
    );
  };

  if (treeMapData.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground">
        <p>No data to display</p>
      </div>
    );
  }

  return (
    <TreeMapContextMenu
      node={contextMenuNode}
      onOpenInFinder={handleOpenInFinder}
      onDelete={handleDeleteClick}
    >
      <div className="h-full w-full relative">
        <ResponsiveContainer width="100%" height="100%">
          <Treemap
            data={treeMapData || []}
            dataKey="size"
            aspectRatio={1}
            content={CustomContent as never}
            isAnimationActive={false}
          />
        </ResponsiveContainer>

        <TreeMapTooltip
          visible={tooltip.visible}
          x={tooltip.x}
          y={tooltip.y}
          data={tooltip.data}
          parentSize={totalSize}
        />

        <DeleteConfirmDialog
          open={deleteDialogOpen}
          node={selectedNode}
          onConfirm={handleDeleteConfirm}
          onCancel={handleDeleteCancel}
        />
      </div>
    </TreeMapContextMenu>
  );
};

export default TreeMapChart;
