import type React from "react";
import { useCallback, useState } from "react";
import { ResponsiveContainer, Treemap } from "recharts";
import { useTreeMapData } from "@/hooks/useTreeMapData";
import { useTreeMapInteraction } from "@/hooks/useTreeMapInteraction";
import { formatFileSize } from "@/utils/formatters";
import { interpolateStops, readableInk, rgbToCss } from "@/lib/colorScale";
import type { FileNode } from "@/types";
import { DeleteConfirmDialog } from "./DeleteConfirmDialog";
import { TreeMapTooltip } from "./TreeMapTooltip";
import { showNodeContextMenu } from "@/hooks/useNativeContextMenu";

interface TreeMapChartProps {
  data: FileNode;
  rampStops: string[];
  onNodeClick?: (node: FileNode) => void;
  onNodeDoubleClick?: (node: FileNode) => void;
  onNodeDeleted?: (node: FileNode) => void;
}

const TreeMapChart: React.FC<TreeMapChartProps> = ({
  data,
  rampStops,
  onNodeClick,
  onNodeDoubleClick,
  onNodeDeleted,
}) => {
  const { data: treeMapData, maxSize, minSize, totalSize } = useTreeMapData(data);
  const { tooltip, handleMouseEnter, handleMouseMove, handleMouseLeave } =
    useTreeMapInteraction();

  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [selectedNode, setSelectedNode] = useState<FileNode | null>(null);

  const handleDeleteClick = useCallback((node: FileNode) => {
    setSelectedNode(node);
    setDeleteDialogOpen(true);
  }, []);

  const handleContextMenuEvent = useCallback(async (node: FileNode) => {
    await showNodeContextMenu(node, handleDeleteClick);
  }, [handleDeleteClick]);

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

    // Continuous size → color mapping using log-normalized t in [0,1]
    const logMin = Math.log(minSize + 1);
    const logMax = Math.log(maxSize + 1);
    const t = logMax > logMin
      ? (Math.log(size + 1) - logMin) / (logMax - logMin)
      : 1;
    const cellRgb = interpolateStops(rampStops, Math.max(0, Math.min(1, t)));
    const fillCss = rgbToCss(cellRgb);
    const inkColor = readableInk(cellRgb);

    const handleClick = (e: React.MouseEvent) => {
      e.preventDefault();
      onNodeClick?.(originalNode);
    };

    const handleDoubleClick = (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      onNodeDoubleClick?.(originalNode);
    };

    const handleRightClick = (e: React.MouseEvent) => {
      e.preventDefault();
      handleContextMenuEvent(originalNode);
    };

    const handleMouseEnterEvent = (e: React.MouseEvent) => {
      handleMouseEnter({ name, size, originalNode }, e);
    };

    const handleKeyDown = (e: React.KeyboardEvent) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        onNodeClick?.(originalNode);
      }
    };

    return (
      <g
        role="button"
        tabIndex={0}
        style={{ cursor: "pointer", outline: "none" }}
        onClick={handleClick}
        onDoubleClick={handleDoubleClick}
        onContextMenu={handleRightClick}
        onMouseEnter={handleMouseEnterEvent}
        onMouseMove={handleMouseMove}
        onMouseLeave={handleMouseLeave}
        onKeyDown={handleKeyDown}
      >
        <rect
          x={adjustedX}
          y={adjustedY}
          width={adjustedWidth}
          height={adjustedHeight}
          fill={fillCss}
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
              fill={inkColor}
              fontSize={fontSize}
              fontWeight="600"
              style={{
                textShadow: `0 1px 2px rgba(0,0,0,0.3)`,
                fontFamily: "var(--font-sans)",
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
                fill={inkColor}
                fontSize={fontSize * 0.75}
                fontWeight="500"
                style={{
                  textShadow: `0 1px 2px rgba(0,0,0,0.3)`,
                  fontFamily: "var(--font-mono)",
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
  );
};

export default TreeMapChart;
