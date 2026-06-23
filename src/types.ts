export interface FileNode {
  id: string;
  name: string;
  path: string;
  type: "file" | "directory";
  size: number;
  /** Total file descendants in this subtree (computed in Rust). */
  fileCount: number;
  /** Total directory descendants in this subtree (computed in Rust). */
  dirCount: number;
  /** Only filled up to the depth requested from `get_subtree`. */
  children?: FileNode[];
  /** Immediate children truncated from `children` (beyond max_children). */
  hiddenChildren?: number;
  /** Summed size of those truncated children, for the "Other" bucket. */
  hiddenSize?: number;
  /** Set on a synthetic "Other" page view: the real node id to paginate. */
  overflowBaseId?: string;
  /** Set on a synthetic "Other" page view: the child offset to load from. */
  overflowOffset?: number;
  lastModified?: number;
  isHidden?: boolean;
  permissions?: string;
}

export interface ScanSummary {
  rootId: string;
  totalSize: number;
  totalFiles: number;
  totalDirectories: number;
  scanDurationMs: number;
}

export interface ScanProgress {
  currentPath: string;
  filesScanned: number;
  directoriesScanned: number;
  bytesScanned: number;
  percent: number;
  isCompleted: boolean;
}
