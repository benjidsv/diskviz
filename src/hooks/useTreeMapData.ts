import { useMemo } from "react";
import type { FileNode } from "@/types";

export interface TreeMapData {
  name: string;
  size: number;
  originalNode: FileNode;
}

/** Sentinel id for the synthetic "Other" tile (not a real, navigable node). */
export const OTHER_NODE_ID = "__other__";

/** True for the aggregated remainder tile — guard drill/delete/context menu. */
export const isOtherNode = (node: FileNode | null | undefined): boolean =>
  node?.id === OTHER_NODE_ID;

export const useTreeMapData = (node: FileNode | null) => {
  const data = useMemo<TreeMapData[]>(() => {
    if (!node || !node.children || node.children.length === 0) {
      return [];
    }
    // The backend already chose which children to show (adaptive cap) and
    // reports the rest via hiddenChildren/hiddenSize — render them all as-is.
    const shown: TreeMapData[] = node.children
      .filter((child) => child.size > 0)
      .sort((a, b) => b.size - a.size)
      .map((child) => ({
        name: child.name,
        size: child.size,
        originalNode: child,
      }));

    // Honest remainder: the backend tells us how many immediate children it
    // truncated and their combined size. Surface them as one "Other" tile that
    // drills into the next page (base node id + offset past what we've shown),
    // so the treemap area still adds up to the directory total and stays explorable.
    if (node.hiddenChildren && node.hiddenChildren > 0 && (node.hiddenSize ?? 0) > 0) {
      const count = node.hiddenChildren;
      const baseId = node.overflowBaseId ?? node.id;
      const nextOffset = (node.overflowOffset ?? 0) + (node.children?.length ?? 0);
      const otherNode: FileNode = {
        id: OTHER_NODE_ID,
        name: `Other (${count.toLocaleString()} items)`,
        path: "",
        type: "directory",
        size: node.hiddenSize ?? 0,
        fileCount: 0,
        dirCount: 0,
        overflowBaseId: baseId,
        overflowOffset: nextOffset,
      };
      shown.push({ name: "Other", size: otherNode.size, originalNode: otherNode });
    }

    return shown;
  }, [node]);

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
