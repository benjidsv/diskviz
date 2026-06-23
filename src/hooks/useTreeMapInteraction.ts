import type React from "react";
import { useCallback, useState } from "react";
import type { FileNode } from "@/types";

export interface TooltipData {
  name: string;
  size: number;
  originalNode: FileNode;
}

interface TooltipState {
  visible: boolean;
  x: number;
  y: number;
  data: TooltipData | null;
}

export function useTreeMapInteraction() {
  const [tooltip, setTooltip] = useState<TooltipState>({
    visible: false,
    x: 0,
    y: 0,
    data: null,
  });

  const handleMouseEnter = useCallback((data: TooltipData, e: React.MouseEvent) => {
    setTooltip({ visible: true, x: e.clientX, y: e.clientY, data });
  }, []);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    setTooltip((prev) => prev.visible ? { ...prev, x: e.clientX, y: e.clientY } : prev);
  }, []);

  const handleMouseLeave = useCallback(() => {
    setTooltip((prev) => ({ ...prev, visible: false }));
  }, []);

  return {
    tooltip,
    handleMouseEnter,
    handleMouseMove,
    handleMouseLeave,
  };
}
