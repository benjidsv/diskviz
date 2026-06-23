import type React from "react";
import { FolderOpen, Trash2 } from "lucide-react";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import type { FileNode } from "@/types";

interface TreeMapContextMenuProps {
  children: React.ReactNode;
  node: FileNode | null;
  onOpenInFinder: (node: FileNode) => void;
  onDelete: (node: FileNode) => void;
}

export const TreeMapContextMenu: React.FC<TreeMapContextMenuProps> = ({
  children,
  node,
  onOpenInFinder,
  onDelete,
}) => {
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      {node && (
        <ContextMenuContent className="w-56">
          <ContextMenuItem onSelect={() => onOpenInFinder(node)}>
            <FolderOpen className="mr-2 h-4 w-4" />
            Show in Finder
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem
            onSelect={() => onDelete(node)}
            className="text-destructive focus:text-destructive"
          >
            <Trash2 className="mr-2 h-4 w-4" />
            Move to Trash
          </ContextMenuItem>
        </ContextMenuContent>
      )}
    </ContextMenu>
  );
};
