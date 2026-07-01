import { Menu, MenuItem, IconMenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
import { NativeIcon } from "@tauri-apps/api/menu/iconMenuItem";
import { openInFinder } from "@/lib/api";
import { isMac, isWindows } from "@/lib/platform";
import type { FileNode } from "@/types";

// "Show in Finder" (macOS) vs "Show in Explorer" (Windows/other). NativeIcon
// maps to macOS NSImage system icons and isn't available on other platforms,
// so non-mac builds fall back to plain (icon-less) MenuItem entries.
const revealLabel = isMac ? "Show in Finder" : "Show in Explorer";
const trashLabel = isWindows ? "Move to Recycle Bin…" : "Move to Trash…";

/**
 * Show a native context menu for a graph node.
 * "Show in Finder/Explorer" runs immediately; "Move to Trash…" calls the
 * provided callback so the caller can open its own confirmation dialog.
 */
export async function showNodeContextMenu(
  node: FileNode,
  onDelete: (node: FileNode) => void,
): Promise<void> {
  const revealAction = async () => {
    try {
      await openInFinder(node.path);
    } catch (e) {
      console.error("Failed to reveal item:", e);
    }
  };

  const finderItem = isMac
    ? await IconMenuItem.new({
        id: "ctx-open-in-finder",
        text: revealLabel,
        icon: NativeIcon.Folder,
        action: revealAction,
      })
    : await MenuItem.new({
        id: "ctx-open-in-finder",
        text: revealLabel,
        action: revealAction,
      });

  const separator = await PredefinedMenuItem.new({ item: "Separator" });

  const trashItem = isMac
    ? await IconMenuItem.new({
        id: "ctx-move-to-trash",
        text: trashLabel,
        icon: NativeIcon.TrashFull,
        action: () => { onDelete(node); },
      })
    : await MenuItem.new({
        id: "ctx-move-to-trash",
        text: trashLabel,
        action: () => { onDelete(node); },
      });

  const menu = await Menu.new({ items: [finderItem, separator, trashItem] });
  await menu.popup();
}

/**
 * Show a native context menu for a breadcrumb segment.
 * Only offers "Show in Finder/Explorer" since breadcrumbs are always directories.
 */
export async function showBreadcrumbContextMenu(node: FileNode): Promise<void> {
  const revealAction = async () => {
    try {
      await openInFinder(node.path);
    } catch (e) {
      console.error("Failed to reveal item:", e);
    }
  };

  const finderItem = isMac
    ? await IconMenuItem.new({
        id: "ctx-crumb-open-in-finder",
        text: revealLabel,
        icon: NativeIcon.Folder,
        action: revealAction,
      })
    : await MenuItem.new({
        id: "ctx-crumb-open-in-finder",
        text: revealLabel,
        action: revealAction,
      });

  const menu = await Menu.new({ items: [finderItem] });
  await menu.popup();
}
