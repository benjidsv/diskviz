import type React from "react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { formatFileSize } from "@/utils/formatters";
import type { FileNode } from "@/types";

interface DeleteConfirmDialogProps {
  open: boolean;
  node: FileNode | null;
  onConfirm: () => void;
  onCancel: () => void;
}

export const DeleteConfirmDialog: React.FC<DeleteConfirmDialogProps> = ({
  open,
  node,
  onConfirm,
  onCancel,
}) => {
  if (!node) return null;

  const isDir = node.type === "directory";

  return (
    <AlertDialog
      open={open}
      onOpenChange={(o) => {
        if (!o) onCancel();
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Move to Trash?</AlertDialogTitle>
          <AlertDialogDescription>
            This moves the following {isDir ? "directory" : "file"} to the system Trash.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <div className="rounded-md bg-muted px-3 py-2 font-mono text-xs break-all">
          {node.path}
        </div>
        <div className="text-sm text-muted-foreground font-mono tabular-nums">Size: {formatFileSize(node.size)}</div>
        {isDir && (
          <div className="text-sm text-destructive">
            This includes the directory and all of its contents
            {node.fileCount > 0 || node.dirCount > 0 ? (
              <span className="font-mono tabular-nums">
                {" "}
                ({node.fileCount.toLocaleString()} files, {node.dirCount.toLocaleString()} folders)
              </span>
            ) : null}
            .
          </div>
        )}
        <AlertDialogFooter>
          <AlertDialogCancel onClick={onCancel}>Cancel</AlertDialogCancel>
          <AlertDialogAction
            onClick={onConfirm}
            className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
          >
            Move to Trash
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
};
