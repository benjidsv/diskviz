import type React from "react";
import { useCallback, useState } from "react";
import { ResponsiveContainer, Treemap } from "recharts";
import { isOtherNode, useTreeMapData } from "@/hooks/useTreeMapData";
import { useTreeMapInteraction } from "@/hooks/useTreeMapInteraction";
import { formatFileSize } from "@/utils/formatters";
import { readableInk, rgbToCss } from "@/lib/colorScale";
import { activenessColorRgb, neutralRgb, sizeColorRgb } from "./vizColor";
import type { ColorMode } from "@/hooks/useVisualizationSettings";
import type { FileNode } from "@/types";
import { DeleteConfirmDialog } from "./DeleteConfirmDialog";
import { TreeMapTooltip } from "./TreeMapTooltip";
import { showNodeContextMenu } from "@/hooks/useNativeContextMenu";

const OUTER_CORNER_RADIUS = 12;
const CORNER_EPSILON = 0.5;

interface CornerRadii {
  tl: number;
  tr: number;
  br: number;
  bl: number;
}

// Rect path with independently-radiused corners: a 0 radius on a corner
// renders as a plain right angle (SVG treats a 0-radius arc as a straight line).
function roundedRectPath(x: number, y: number, width: number, height: number, r: CornerRadii) {
  const { tl, tr, br, bl } = r;
  return [
    `M ${x + tl},${y}`,
    `H ${x + width - tr}`,
    `A ${tr},${tr} 0 0 1 ${x + width},${y + tr}`,
    `V ${y + height - br}`,
    `A ${br},${br} 0 0 1 ${x + width - br},${y + height}`,
    `H ${x + bl}`,
    `A ${bl},${bl} 0 0 1 ${x},${y + height - bl}`,
    `V ${y + tl}`,
    `A ${tl},${tl} 0 0 1 ${x + tl},${y}`,
    "Z",
  ].join(" ");
}

interface TreeMapChartProps {
  data: FileNode;
  rampStops: string[];
  colorMode: ColorMode;
  ageRampStops: string[];
  ageThresholdDays: number;
  selectedId?: string;
  onNodeSelect?: (node: FileNode | null) => void;
  onNodeDoubleClick?: (node: FileNode) => void;
  onNodeDeleted?: (node: FileNode) => void;
}

const TreeMapChart: React.FC<TreeMapChartProps> = ({
  data,
  rampStops,
  colorMode,
  ageRampStops,
  ageThresholdDays,
  selectedId,
  onNodeSelect,
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
    root: { x: number; y: number; width: number; height: number };
  }

  const CustomContent: React.FC<CustomContentProps> = ({
    x,
    y,
    width,
    height,
    name,
    size,
    originalNode,
    root,
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

    // Only the tile occupying a given outer corner of the whole treemap gets
    // that one corner rounded; every other corner/tile stays square.
    const atLeft = Math.abs(x - root.x) < CORNER_EPSILON;
    const atTop = Math.abs(y - root.y) < CORNER_EPSILON;
    const atRight = Math.abs(x + width - (root.x + root.width)) < CORNER_EPSILON;
    const atBottom = Math.abs(y + height - (root.y + root.height)) < CORNER_EPSILON;
    const cornerRadius = Math.max(
      0,
      Math.min(OUTER_CORNER_RADIUS, adjustedWidth / 2, adjustedHeight / 2),
    );
    const radii: CornerRadii = {
      tl: atLeft && atTop ? cornerRadius : 0,
      tr: atRight && atTop ? cornerRadius : 0,
      br: atRight && atBottom ? cornerRadius : 0,
      bl: atLeft && atBottom ? cornerRadius : 0,
    };
    const hasRoundedCorner = radii.tl > 0 || radii.tr > 0 || radii.br > 0 || radii.bl > 0;

    const isSmall = adjustedWidth < 40 || adjustedHeight < 20;
    const fontSize = Math.max(
      isSmall ? 8 : 10,
      Math.min(adjustedWidth / 6, adjustedHeight / 3, 16),
    );
    const shouldShowText = adjustedWidth > 30 && adjustedHeight > 20;

    const isOther = isOtherNode(originalNode);
    const cellRgb =
      colorMode === "activeness"
        ? (!isOther &&
            activenessColorRgb(originalNode.medianMtime, ageRampStops, ageThresholdDays)) ||
          neutralRgb(rampStops)
        : sizeColorRgb(size, minSize, maxSize, rampStops);
    const fillCss = rgbToCss(cellRgb);
    const inkColor = readableInk(cellRgb);
    const isSelected = !!selectedId && originalNode.id === selectedId;

    const handleClick = (e: React.MouseEvent) => {
      e.preventDefault();
      onNodeSelect?.(isSelected ? null : originalNode);
    };

    const handleDoubleClick = (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      // "Other" is drillable: App pages into the next batch of children.
      onNodeDoubleClick?.(originalNode);
    };

    const handleRightClick = (e: React.MouseEvent) => {
      e.preventDefault();
      if (isOther) return; // synthetic group — no single path to reveal/trash
      handleContextMenuEvent(originalNode);
    };

    const handleMouseEnterEvent = (e: React.MouseEvent) => {
      handleMouseEnter({ name, size, originalNode }, e);
    };

    const handleKeyDown = (e: React.KeyboardEvent) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        onNodeSelect?.(isSelected ? null : originalNode);
      }
    };

    // Truncate to fit, using a proper ellipsis and a little horizontal padding.
    const maxChars = Math.max(1, Math.floor((adjustedWidth - 8) / (fontSize * 0.58)));
    const label = name.length > maxChars ? `${name.slice(0, Math.max(1, maxChars - 1))}…` : name;

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
        {hasRoundedCorner ? (
          <path
            d={roundedRectPath(adjustedX, adjustedY, adjustedWidth, adjustedHeight, radii)}
            fill={fillCss}
            stroke={isSelected ? "var(--primary)" : "var(--viz-stroke)"}
            strokeWidth={isSelected ? 2 : isSmall ? 0.5 : 1}
          />
        ) : (
          <rect
            x={adjustedX}
            y={adjustedY}
            width={adjustedWidth}
            height={adjustedHeight}
            fill={fillCss}
            stroke={isSelected ? "var(--primary)" : "var(--viz-stroke)"}
            strokeWidth={isSelected ? 2 : isSmall ? 0.5 : 1}
          />
        )}

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
              {label}
            </text>

            {adjustedHeight > 40 && adjustedWidth > 40 && (
              <text
                x={adjustedX + adjustedWidth / 2}
                y={adjustedY + adjustedHeight / 2 + fontSize / 2 + 4}
                textAnchor="middle"
                fill={inkColor}
                fontSize={Math.max(9, fontSize * 0.75)}
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
