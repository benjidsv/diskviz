import { Menu, IconMenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
import { NativeIcon } from "@tauri-apps/api/menu/iconMenuItem";
import { openInFinder } from "@/lib/api";
import type { FileNode } from "@/types";

/**
 * Show a native macOS context menu for a graph node.
 * "Show in Finder" runs immediately; "Move to Trash…" calls the provided
 * callback so the caller can open its own confirmation dialog.
 */
export async function showNodeContextMenu(
  node: FileNode,
  onDelete: (node: FileNode) => void,
): Promise<void> {
  const finderItem = await IconMenuItem.new({
    id: "ctx-open-in-finder",
    text: "Show in Finder",
    icon: NativeIcon.Folder,
    action: async () => {
      try {
        await openInFinder(node.path);
      } catch (e) {
        console.error("Failed to open in Finder:", e);
      }
    },
  });

  const separator = await PredefinedMenuItem.new({ item: "Separator" });

  const trashItem = await IconMenuItem.new({
    id: "ctx-move-to-trash",
    text: "Move to Trash…",
    icon: NativeIcon.TrashFull,
    action: () => {
      onDelete(node);
    },
  });

  const menu = await Menu.new({ items: [finderItem, separator, trashItem] });
  await menu.popup();
}

/**
 * Show a native macOS context menu for a breadcrumb segment.
 * Only offers "Show in Finder" since breadcrumbs are always directories.
 */
export async function showBreadcrumbContextMenu(node: FileNode): Promise<void> {
  const finderItem = await IconMenuItem.new({
    id: "ctx-crumb-open-in-finder",
    text: "Show in Finder",
    icon: NativeIcon.Folder,
    action: async () => {
      try {
        await openInFinder(node.path);
      } catch (e) {
        console.error("Failed to open in Finder:", e);
      }
    },
  });

  const menu = await Menu.new({ items: [finderItem] });
  await menu.popup();
}
