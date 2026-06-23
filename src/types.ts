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
