import { BarChart3, HardDriveIcon, KeyboardIcon, Target, XIcon } from "lucide-react";
import React, { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useVisualizationSettings } from "@/hooks/useVisualizationSettings";
import { useThemeSettings } from "@/hooks/useThemeSettings";
import type { ThemeSetting } from "@/hooks/useThemeSettings";
import { formatDuration, formatFileSize } from "@/utils/formatters";
import {
  deleteNode,
  getHomeDirectory,
  getSubtree,
  onScanProgress,
  pickDirectory,
  scanDirectory,
} from "@/lib/api";
import type { FileNode, ScanProgress as Progress, ScanSummary } from "@/types";
import SunburstChart from "@/components/charts/SunburstChart";
import TreeMapChart from "@/components/charts/TreeMapChart";
import ScanProgress from "@/components/ScanProgress";

function App() {
  const [currentPath, setCurrentPath] = useState<string>("");
  const [summary, setSummary] = useState<ScanSummary | null>(null);
  const [isScanning, setIsScanning] = useState(false);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [currentViewNode, setCurrentViewNode] = useState<FileNode | null>(null);
  const [breadcrumbs, setBreadcrumbs] = useState<FileNode[]>([]);
  const [showShortcuts, setShowShortcuts] = useState(false);
  const { visualizationType, setVisualizationType } = useVisualizationSettings();
  const { theme, setTheme } = useThemeSettings();

  // The path actually scanned (used for re-scan after delete).
  const scannedPath = useRef<string>("");

  useEffect(() => {
    getHomeDirectory()
      .then((home) => setCurrentPath((prev) => prev || home))
      .catch((e) => console.error("Failed to get home directory:", e));
  }, []);

  // Subscribe to streamed scan progress for the duration of the app.
  useEffect(() => {
    const unlisten = onScanProgress(setProgress);
    return () => {
      unlisten.then((fn) => fn()).catch(() => {});
    };
  }, []);

  const handleFolderPicker = useCallback(async () => {
    try {
      const selected = await pickDirectory();
      if (selected) setCurrentPath(selected);
    } catch (error) {
      console.error("Failed to open directory dialog:", error);
    }
  }, []);

  const handleScanDirectory = useCallback(
    async (path?: string) => {
      const pathToScan = path || currentPath;
      if (!pathToScan) return;

      setSummary(null);
      setCurrentViewNode(null);
      setBreadcrumbs([]);
      setProgress(null);
      setIsScanning(true);
      scannedPath.current = pathToScan;

      try {
        const result = await scanDirectory(pathToScan);
        const root = await getSubtree(result.rootId);
        setSummary(result);
        setCurrentViewNode(root);
        setBreadcrumbs([root]);
      } catch (error) {
        console.error("Scan failed:", error);
      } finally {
        setIsScanning(false);
      }
    },
    [currentPath],
  );

  const handleNodeDoubleClick = useCallback(async (node: FileNode) => {
    if (node.type !== "directory") return;
    try {
      const fresh = await getSubtree(node.id);
      if (!fresh.children || fresh.children.length === 0) return;
      setCurrentViewNode(fresh);
      setBreadcrumbs((prev) => [...prev, fresh]);
    } catch (error) {
      console.error("Failed to load directory:", error);
    }
  }, []);

  const handleBreadcrumbClick = useCallback(
    async (index: number) => {
      const target = breadcrumbs[index];
      if (!target) return;
      try {
        const fresh = await getSubtree(target.id);
        setBreadcrumbs((prev) => prev.slice(0, index + 1));
        setCurrentViewNode(fresh);
      } catch (error) {
        console.error("Failed to navigate:", error);
      }
    },
    [breadcrumbs],
  );

  // Incremental delete: subtract the node's size from ancestors in the Rust
  // arena, then re-fetch only the current view — no full rescan needed.
  const handleNodeDeleted = useCallback(
    async (node: FileNode) => {
      if (!currentViewNode) return;
      try {
        const newSummary = await deleteNode(node.id);
        setSummary(newSummary);
        // Re-fetch the current view to see the removed node and updated sizes.
        const fresh = await getSubtree(currentViewNode.id);
        setCurrentViewNode(fresh);
      } catch (error) {
        console.error("Incremental delete failed, falling back to full rescan:", error);
        void handleScanDirectory(scannedPath.current);
      }
    },
    [currentViewNode, handleScanDirectory],
  );

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey) {
        switch (event.key) {
          case "o":
            event.preventDefault();
            handleFolderPicker();
            break;
          case "/":
          case "?":
            event.preventDefault();
            setShowShortcuts((s) => !s);
            break;
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleFolderPicker]);

  return (
    <div className="h-screen bg-background overflow-hidden flex flex-col">
      <main className="container mx-auto px-8 py-6 max-w-6xl flex-1 flex flex-col overflow-hidden min-h-0">
        {!isScanning && (
          <div className="flex items-center justify-center">
            <div className="flex items-center space-x-4 w-full max-w-2xl">
              <Input
                value={currentPath}
                onChange={(e) => setCurrentPath(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleScanDirectory();
                }}
                placeholder="Select a directory to analyze"
                className="flex-1 h-12 text-sm"
              />
              <Button onClick={handleFolderPicker} variant="outline" className="h-12 px-4">
                Browse
              </Button>
              <Button
                onClick={() => handleScanDirectory()}
                disabled={isScanning || !currentPath}
                className="h-12 px-6 bg-primary hover:bg-primary/90"
              >
                Analyze
              </Button>
            </div>
          </div>
        )}

        {isScanning && (
          <div className="flex items-center justify-center flex-1">
            <ScanProgress progress={progress} rootPath={scannedPath.current} />
          </div>
        )}

        {summary && currentViewNode && (
          <div className="flex-1 flex flex-col space-y-4 mt-4 overflow-hidden">
            <div className="flex items-center justify-between">
              {breadcrumbs.length > 1 && (
                <div className="flex items-center space-x-2 text-sm text-muted-foreground overflow-x-auto">
                  {breadcrumbs.map((crumb, index) => (
                    <React.Fragment key={crumb.id}>
                      <button
                        type="button"
                        onClick={() => handleBreadcrumbClick(index)}
                        className={`hover:text-foreground transition-colors whitespace-nowrap ${
                          index === breadcrumbs.length - 1
                            ? "text-foreground font-medium"
                            : "hover:text-foreground"
                        }`}
                      >
                        {crumb.name}
                      </button>
                      {index < breadcrumbs.length - 1 && <span>/</span>}
                    </React.Fragment>
                  ))}
                </div>
              )}

              <div className="flex items-center space-x-4 ml-auto">
                <div className="flex items-center space-x-2">
                  <span className="text-sm text-muted-foreground">Theme:</span>
                  <select
                    value={theme}
                    onChange={(e) => setTheme(e.target.value as ThemeSetting)}
                    className="text-sm border border-border rounded px-2 py-1 bg-background text-foreground cursor-pointer"
                  >
                    <option value="system">System</option>
                    <option value="latte">Latte</option>
                    <option value="frappe">Frappé</option>
                    <option value="macchiato">Macchiato</option>
                    <option value="mocha">Mocha</option>
                  </select>
                </div>
                <div className="flex items-center space-x-2">
                  <span className="text-sm text-muted-foreground">View:</span>
                  <ToggleGroup
                    type="single"
                    value={visualizationType}
                    onValueChange={(value) => {
                      if (value === "treemap" || value === "sunburst") {
                        setVisualizationType(value);
                      }
                    }}
                    variant="outline"
                    size="sm"
                  >
                    <ToggleGroupItem value="treemap" aria-label="TreeMap view">
                      <BarChart3 className="h-4 w-4" />
                    </ToggleGroupItem>
                    <ToggleGroupItem value="sunburst" aria-label="Sunburst view">
                      <Target className="h-4 w-4" />
                    </ToggleGroupItem>
                  </ToggleGroup>
                </div>
              </div>
            </div>

            <div className="grid grid-cols-3 gap-8 flex-1 min-h-0">
              <div className="col-span-2 h-full border border-border/20 rounded-lg p-4">
                {visualizationType === "treemap" ? (
                  <TreeMapChart
                    data={currentViewNode}
                    onNodeDoubleClick={handleNodeDoubleClick}
                    onNodeDeleted={handleNodeDeleted}
                  />
                ) : (
                  <SunburstChart
                    data={currentViewNode}
                    onNodeDoubleClick={handleNodeDoubleClick}
                    onNodeDeleted={handleNodeDeleted}
                  />
                )}
              </div>

              <div className="col-span-1 space-y-6 overflow-y-auto">
                <div className="text-center space-y-2 p-6 rounded-lg border border-border/20">
                  <div className="text-4xl font-bold text-foreground tabular-nums">
                    {formatFileSize(currentViewNode.size)}
                  </div>
                  <div className="text-sm text-muted-foreground font-medium">Total Size</div>
                </div>

                <div className="text-center space-y-2 p-6 rounded-lg border border-border/20">
                  <div className="text-4xl font-bold text-foreground tabular-nums">
                    {currentViewNode.fileCount.toLocaleString()}
                  </div>
                  <div className="text-sm text-muted-foreground font-medium">Files</div>
                </div>

                <div className="text-center space-y-2 p-6 rounded-lg border border-border/20">
                  <div className="text-4xl font-bold text-foreground tabular-nums">
                    {currentViewNode.dirCount.toLocaleString()}
                  </div>
                  <div className="text-sm text-muted-foreground font-medium">Directories</div>
                </div>

                <div className="text-center space-y-2 p-6 rounded-lg border border-border/20">
                  <div className="text-4xl font-bold text-foreground tabular-nums">
                    {formatDuration(summary.scanDurationMs)}
                  </div>
                  <div className="text-sm text-muted-foreground font-medium">Scan Time</div>
                </div>
              </div>
            </div>
          </div>
        )}

        {!summary && !isScanning && (
          <div className="flex flex-col items-center justify-center flex-1 space-y-4">
            <div className="bg-muted/30 p-6 rounded-full">
              <HardDriveIcon className="h-12 w-12 text-muted-foreground" />
            </div>
            <div className="text-center space-y-2">
              <h3 className="text-xl font-medium text-foreground">Choose a directory</h3>
              <p className="text-muted-foreground">
                Select a folder above to visualize what's using your space
              </p>
            </div>
          </div>
        )}
      </main>

      <footer className="border-t border-border/10 px-6 py-3 flex-shrink-0 bg-muted/30">
        <div className="flex items-center justify-between text-xs text-muted-foreground">
          <div className="flex items-center space-x-2">
            <span className="font-medium">diskviz</span>
            <span className="text-muted-foreground/60">fast disk usage visualizer</span>
          </div>
          <div className="flex items-center space-x-3">
            <button
              type="button"
              onClick={() => setShowShortcuts((s) => !s)}
              className="flex items-center space-x-1 hover:text-foreground transition-colors"
              title="Keyboard shortcuts (Cmd+?)"
            >
              <KeyboardIcon className="w-3 h-3" />
              <span>Shortcuts</span>
            </button>
            <span className="text-muted-foreground/60">UI ported from vizdisk (MIT)</span>
          </div>
        </div>
      </footer>

      {showShortcuts && (
        <div
          role="button"
          tabIndex={0}
          className="fixed inset-0 bg-black/50 flex items-center justify-center z-[10000]"
          onClick={() => setShowShortcuts(false)}
          onKeyDown={(e) => e.key === "Escape" && setShowShortcuts(false)}
        >
          <div
            role="dialog"
            className="bg-background border border-border rounded-lg shadow-xl p-6 max-w-md w-full mx-4"
            onClick={(e) => e.stopPropagation()}
            onKeyDown={(e) => e.key === "Enter" && e.stopPropagation()}
          >
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-lg font-semibold">Keyboard Shortcuts</h3>
              <button
                type="button"
                onClick={() => setShowShortcuts(false)}
                className="text-muted-foreground hover:text-foreground transition-colors"
              >
                <XIcon className="w-5 h-5" />
              </button>
            </div>
            <div className="space-y-3 text-sm">
              <div className="flex justify-between items-center">
                <span className="text-muted-foreground">Open folder</span>
                <kbd className="bg-muted px-2 py-1 rounded text-xs font-mono">⌘O</kbd>
              </div>
              <div className="flex justify-between items-center">
                <span className="text-muted-foreground">Show shortcuts</span>
                <kbd className="bg-muted px-2 py-1 rounded text-xs font-mono">⌘?</kbd>
              </div>
              <div className="flex justify-between items-center">
                <span className="text-muted-foreground">Double-click folder</span>
                <span className="text-xs text-muted-foreground">Drill down</span>
              </div>
              <div className="flex justify-between items-center">
                <span className="text-muted-foreground">Breadcrumb navigation</span>
                <span className="text-xs text-muted-foreground">Go back</span>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
