import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { FileNode, ScanProgress, ScanSummary } from "@/types";

/** Scan a directory. Totals come back here; progress streams via onScanProgress. */
export const scanDirectory = (path: string) =>
  invoke<ScanSummary>("scan_directory", { path });

/** Request cancellation of the in-flight scan. Rejects the scanDirectory call. */
export const cancelScan = () => invoke<void>("cancel_scan");

/** Pull a bounded slice of the scanned tree (kept in Rust) for rendering.
 * `offset` paginates the node's size-sorted children (for the "Other" bucket). */
export const getSubtree = (nodeId: string, maxDepth = 3, maxChildren = 20, offset = 0) =>
  invoke<FileNode>("get_subtree", { nodeId, maxDepth, maxChildren, offset });

export const getHomeDirectory = () => invoke<string>("get_home_directory");

export const getCommonDirectories = () =>
  invoke<string[]>("get_common_directories");

export const validatePath = (path: string) =>
  invoke<boolean>("validate_path", { path });

export const deletePath = (path: string) =>
  invoke<void>("delete_path", { path });

/** Delete a node by arena id. Updates the in-memory tree and returns new totals. */
export const deleteNode = (nodeId: string) =>
  invoke<ScanSummary>("delete_node", { nodeId });

export const openInFinder = (path: string) =>
  invoke<void>("open_in_finder", { path });

/** Subscribe to streamed scan progress. Returns an unlisten fn. */
export const onScanProgress = (
  cb: (p: ScanProgress) => void,
): Promise<UnlistenFn> =>
  listen<ScanProgress>("scan-progress", (e) => cb(e.payload));

/** Native directory picker. */
export const pickDirectory = () =>
  open({ directory: true, multiple: false }) as Promise<string | null>;
