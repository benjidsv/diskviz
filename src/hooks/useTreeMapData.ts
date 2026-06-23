import { useMemo } from "react";
import type { FileNode } from "@/types";

export interface TreeMapData {
  name: string;
  size: number;
  originalNode: FileNode;
}

export const useTreeMapData = (node: FileNode | null, maxItems = 20) => {
  const data = useMemo<TreeMapData[]>(() => {
    if (!node || !node.children || node.children.length === 0) {
      return [];
    }
    return node.children
      .filter((child) => child.size > 0)
      .sort((a, b) => b.size - a.size)
      .slice(0, maxItems)
      .map((child) => ({
        name: child.name,
        size: child.size,
        originalNode: child,
      }));
  }, [node, maxItems]);

  const maxSize = useMemo(() => {
    if (data.length === 0) return 0;
    return Math.max(...data.map((d) => d.size));
  }, [data]);

  const minSize = useMemo(() => {
    if (data.length === 0) return 0;
    return Math.min(...data.map((d) => d.size));
  }, [data]);

  const totalSize = useMemo(() => node?.size || 0, [node]);

  return { data, maxSize, minSize, totalSize };
};
