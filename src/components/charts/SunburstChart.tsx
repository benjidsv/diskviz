import type React from "react";
import { useCallback, useMemo, useState } from "react";
import { formatFileSize } from "@/utils/formatters";
import { openInFinder } from "@/lib/api";
import { hexToRgb, readableInk } from "@/lib/colorScale";
import { useThemeSettings, VIZ_SUN_COLORS } from "@/hooks/useThemeSettings";
import type { FileNode } from "@/types";
import { DeleteConfirmDialog } from "./DeleteConfirmDialog";
import { TreeMapContextMenu } from "./TreeMapContextMenu";
import { TreeMapTooltip } from "./TreeMapTooltip";

interface SunburstChartProps {
  data: FileNode;
  onNodeClick?: (node: FileNode) => void;
  onNodeDoubleClick?: (node: FileNode) => void;
  onNodeDeleted?: (node: FileNode) => void;
}

interface SunburstNode {
  node: FileNode;
  startAngle: number;
  endAngle: number;
  innerRadius: number;
  outerRadius: number;
  level: number;
}

const SunburstChart: React.FC<SunburstChartProps> = ({
  data,
  onNodeDoubleClick,
  onNodeDeleted,
}) => {
  const { resolvedFlavor } = useThemeSettings();
  const [tooltip, setTooltip] = useState<{
    visible: boolean;
    x: number;
    y: number;
    data: { name: string; size: number; originalNode: FileNode } | null;
  }>({
    visible: false,
    x: 0,
    y: 0,
    data: null,
  });

  const [contextMenuNode, setContextMenuNode] = useState<FileNode | null>(null);
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [selectedNode, setSelectedNode] = useState<FileNode | null>(null);

  const centerX = 300;
  const centerY = 300;
  const maxRadius = 280;
  const minRadius = 50;

  // Cycle through Catppuccin accent colors per depth level. The arc area
  // already communicates size; level color adds structural context.
  const getColor = useCallback((_size: number, _max: number, level: number): string => {
    return `var(--viz-sun-${level % 7})`;
  }, []);

  // Pre-resolve arc fill hex per level for readable label ink calculation.
  const arcInkColors = useMemo(
    () => VIZ_SUN_COLORS[resolvedFlavor].map((hex) => readableInk(hexToRgb(hex))),
    [resolvedFlavor],
  );

  const calculateSunburstNodes = useCallback(
    (
      node: FileNode,
      startAngle: number,
      endAngle: number,
      innerRadius: number,
      outerRadius: number,
      level: number,
      totalSize: number,
      maxLevel = 4,
    ): SunburstNode[] => {
      if (level > maxLevel || !node.children || node.children.length === 0) {
        return [{ node, startAngle, endAngle, innerRadius, outerRadius, level }];
      }

      const nodes: SunburstNode[] = [
        { node, startAngle, endAngle, innerRadius, outerRadius, level },
      ];

      const angleRange = endAngle - startAngle;
      const radiusStep = (maxRadius - minRadius) / maxLevel;
      const childInnerRadius = outerRadius;
      const childOuterRadius = Math.min(maxRadius, outerRadius + radiusStep);

      let currentAngle = startAngle;

      const sortedChildren = [...node.children].sort((a, b) => b.size - a.size);

      for (const child of sortedChildren) {
        if (child.size === 0) continue;

        const childAngleRange = (child.size / node.size) * angleRange;
        const childEndAngle = currentAngle + childAngleRange;

        nodes.push(
          ...calculateSunburstNodes(
            child,
            currentAngle,
            childEndAngle,
            childInnerRadius,
            childOuterRadius,
            level + 1,
            totalSize,
            maxLevel,
          ),
        );

        currentAngle = childEndAngle;
      }

      return nodes;
    },
    [],
  );

  const createArcPath = useCallback(
    (startAngle: number, endAngle: number, innerRadius: number, outerRadius: number): string => {
      const startAngleRad = (startAngle - 90) * (Math.PI / 180);
      const endAngleRad = (endAngle - 90) * (Math.PI / 180);

      const x1 = centerX + outerRadius * Math.cos(startAngleRad);
      const y1 = centerY + outerRadius * Math.sin(startAngleRad);
      const x2 = centerX + outerRadius * Math.cos(endAngleRad);
      const y2 = centerY + outerRadius * Math.sin(endAngleRad);

      const x3 = centerX + innerRadius * Math.cos(endAngleRad);
      const y3 = centerY + innerRadius * Math.sin(endAngleRad);
      const x4 = centerX + innerRadius * Math.cos(startAngleRad);
      const y4 = centerY + innerRadius * Math.sin(startAngleRad);

      const largeArcFlag = endAngle - startAngle <= 180 ? "0" : "1";

      return [
        `M ${x1} ${y1}`,
        `A ${outerRadius} ${outerRadius} 0 ${largeArcFlag} 1 ${x2} ${y2}`,
        `L ${x3} ${y3}`,
        `A ${innerRadius} ${innerRadius} 0 ${largeArcFlag} 0 ${x4} ${y4}`,
        "Z",
      ].join(" ");
    },
    [],
  );

  const handleMouseEnter = useCallback((sunburstNode: SunburstNode, e: React.MouseEvent) => {
    setTooltip({
      visible: true,
      x: e.clientX,
      y: e.clientY,
      data: {
        name: sunburstNode.node.name,
        size: sunburstNode.node.size,
        originalNode: sunburstNode.node,
      },
    });
  }, []);

  const handleMouseLeave = useCallback(() => {
    setTooltip((prev) => ({ ...prev, visible: false }));
  }, []);

  const handleContextMenu = useCallback((node: FileNode) => {
    setContextMenuNode(node);
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

  if (!data || data.size === 0) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground">
        <p>No data to display</p>
      </div>
    );
  }

  const sunburstNodes = calculateSunburstNodes(
    data,
    0,
    360,
    minRadius,
    minRadius + (maxRadius - minRadius) / 4,
    0,
    data.size,
  );
  const maxSize = data.size;

  return (
    <TreeMapContextMenu
      node={contextMenuNode}
      onOpenInFinder={handleOpenInFinder}
      onDelete={handleDeleteClick}
    >
      <div className="h-full w-full relative flex items-center justify-center">
        <svg
          viewBox="0 0 600 600"
          className="max-h-full max-w-full"
          role="img"
          aria-label="Sunburst chart showing disk usage"
        >
          <title>Disk usage sunburst chart</title>
          {sunburstNodes.map((sunburstNode, index) => {
            const { node, startAngle, endAngle, innerRadius, outerRadius, level } = sunburstNode;

            if (endAngle - startAngle < 1) return null;

            const arcPath = createArcPath(startAngle, endAngle, innerRadius, outerRadius);
            const color = getColor(node.size, maxSize, level);

            const midAngle = (startAngle + endAngle) / 2;
            const midAngleRad = (midAngle - 90) * (Math.PI / 180);
            const textRadius = (innerRadius + outerRadius) / 2;
            const textX = centerX + textRadius * Math.cos(midAngleRad);
            const textY = centerY + textRadius * Math.sin(midAngleRad);

            const shouldShowText = endAngle - startAngle > 15 && outerRadius - innerRadius > 20;
            const fontSize = Math.min(12, (endAngle - startAngle) / 5, (outerRadius - innerRadius) / 3);

            return (
              <g key={`${node.path}-${index}`}>
                <path
                  d={arcPath}
                  fill={color}
                  stroke="var(--viz-stroke)"
                  strokeWidth={1}
                  className="cursor-pointer transition-all duration-300 hover:brightness-110"
                  style={{ outline: "none" }}
                  aria-label={`${node.name} - ${formatFileSize(node.size)}`}
                  onDoubleClick={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    onNodeDoubleClick?.(node);
                  }}
                  onContextMenu={(_e) => {
                    handleContextMenu(node);
                  }}
                  onMouseEnter={(e) => handleMouseEnter(sunburstNode, e)}
                  onMouseMove={(e) => setTooltip((prev) => prev.visible ? { ...prev, x: e.clientX, y: e.clientY } : prev)}
                  onMouseLeave={handleMouseLeave}
                />

                {shouldShowText && (
                  <text
                    x={textX}
                    y={textY}
                    textAnchor="middle"
                    dominantBaseline="middle"
                    fill={arcInkColors[level % 7]}
                    fontSize={fontSize}
                    fontWeight="600"
                    className="pointer-events-none select-none"
                    style={{
                      textShadow: "0 1px 2px rgba(0,0,0,0.3)",
                      fontFamily: "system-ui, -apple-system, sans-serif",
                    }}
                    transform={`rotate(${midAngle > 90 && midAngle < 270 ? midAngle + 180 : midAngle}, ${textX}, ${textY})`}
                  >
                    {node.name.length > 8 ? `${node.name.substring(0, 8)}...` : node.name}
                  </text>
                )}
              </g>
            );
          })}

          <circle
            cx={centerX}
            cy={centerY}
            r={minRadius}
            fill="var(--background)"
            stroke="var(--viz-stroke)"
            strokeWidth={2}
          />
          <text
            x={centerX}
            y={centerY - 5}
            textAnchor="middle"
            dominantBaseline="middle"
            fill="var(--foreground)"
            fontSize={14}
            fontWeight="600"
            className="pointer-events-none select-none"
            style={{ fontFamily: "system-ui, -apple-system, sans-serif" }}
          >
            {data.name}
          </text>
          <text
            x={centerX}
            y={centerY + 10}
            textAnchor="middle"
            dominantBaseline="middle"
            fill="var(--muted-foreground)"
            fontSize={10}
            fontWeight="500"
            className="pointer-events-none select-none"
            style={{ fontFamily: "system-ui, -apple-system, sans-serif" }}
          >
            {formatFileSize(data.size)}
          </text>
        </svg>

        <TreeMapTooltip
          visible={tooltip.visible}
          x={tooltip.x}
          y={tooltip.y}
          data={tooltip.data}
          parentSize={data.size}
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

export default SunburstChart;
