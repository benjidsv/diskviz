import { BarChart3, HardDriveIcon, InfoIcon, KeyboardIcon, Target, XIcon } from "lucide-react";
import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useVisualizationSettings } from "@/hooks/useVisualizationSettings";
import { useThemeSettings, VIZ_RAMP_BASE } from "@/hooks/useThemeSettings";
import type { ThemeSetting, AccentColor } from "@/hooks/useThemeSettings";
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
import NoticesModal from "@/components/NoticesModal";
import { AccentPicker } from "@/components/AccentPicker";
import { ThemePicker } from "@/components/ThemePicker";

function App() {
  const [currentPath, setCurrentPath] = useState<string>("");
  const [summary, setSummary] = useState<ScanSummary | null>(null);
  const [isScanning, setIsScanning] = useState(false);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [currentViewNode, setCurrentViewNode] = useState<FileNode | null>(null);
  const [breadcrumbs, setBreadcrumbs] = useState<FileNode[]>([]);
  const [showShortcuts, setShowShortcuts] = useState(false);
  const [showNotices, setShowNotices] = useState(false);
  const { visualizationType, setVisualizationType } = useVisualizationSettings();
  const { theme, setTheme, accent, setAccent, resolvedFlavor, accentColor } = useThemeSettings();

  const scannedPath = useRef<string>("");
  const closeButtonRef = useRef<HTMLButtonElement>(null);

  // Full 5-stop ramp: 4 static base stops + current accent as ramp-5
  const rampStops = useMemo(
    () => [...VIZ_RAMP_BASE[resolvedFlavor], accentColor] as string[],
    [resolvedFlavor, accentColor],
  );

  useEffect(() => {
    getHomeDirectory()
      .then((home) => setCurrentPath((prev) => prev || home))
      .catch((e) => console.error("Failed to get home directory:", e));
  }, []);

  useEffect(() => {
    const unlisten = onScanProgress(setProgress);
    return () => {
      unlisten.then((fn) => fn()).catch(() => {});
    };
  }, []);

  // Autofocus the close button when the shortcuts modal opens
  useEffect(() => {
    if (showShortcuts) {
      closeButtonRef.current?.focus();
    }
  }, [showShortcuts]);

  // Global Escape handler for the shortcuts modal
  useEffect(() => {
    if (!showShortcuts) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") setShowShortcuts(false);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [showShortcuts]);

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

  const handleNodeDeleted = useCallback(
    async (node: FileNode) => {
      if (!currentViewNode) return;
      try {
        const newSummary = await deleteNode(node.id);
        setSummary(newSummary);
        const fresh = await getSubtree(currentViewNode.id);
        setCurrentViewNode(fresh);
      } catch (error) {
        console.error("Incremental delete failed, falling back to full rescan:", error);
        void handleScanDirectory(scannedPath.current);
      }
    },
    [currentViewNode, handleScanDirectory],
  );

  const handleGoBack = useCallback(() => {
    if (breadcrumbs.length <= 1) return;
    handleBreadcrumbClick(breadcrumbs.length - 2);
  }, [breadcrumbs.length, handleBreadcrumbClick]);

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
      if (event.key === "Backspace" && !showShortcuts) {
        handleGoBack();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleFolderPicker, handleGoBack, showShortcuts]);

  // Touch tracking for swipe-back gesture
  const touchStartX = useRef(0);
  const touchStartY = useRef(0);

  const handleTouchStart = useCallback((e: TouchEvent) => {
    touchStartX.current = e.touches[0].clientX;
    touchStartY.current = e.touches[0].clientY;
  }, []);

  const handleTouchEnd = useCallback(
    (e: TouchEvent) => {
      const deltaX = e.changedTouches[0].clientX - touchStartX.current;
      const deltaY = e.changedTouches[0].clientY - touchStartY.current;
      const horizontalDistance = Math.abs(deltaX);
      const verticalDistance = Math.abs(deltaY);
      // Swipe back: rightward motion > 50px, more horizontal than vertical
      if (
        deltaX > 50 &&
        horizontalDistance > verticalDistance &&
        verticalDistance < 50
      ) {
        handleGoBack();
      }
    },
    [handleGoBack],
  );

  useEffect(() => {
    window.addEventListener("touchstart", handleTouchStart, false);
    window.addEventListener("touchend", handleTouchEnd, false);
    return () => {
      window.removeEventListener("touchstart", handleTouchStart, false);
      window.removeEventListener("touchend", handleTouchEnd, false);
    };
  }, [handleTouchStart, handleTouchEnd]);

  return (
    <div className="h-screen bg-background overflow-hidden flex flex-col">
      <main className="flex-1 flex flex-col overflow-hidden min-h-0 px-8 pt-6 pb-3">
        {!isScanning && (
          <div className="flex items-center justify-center mb-4 flex-shrink-0">
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
          <div className="flex-1 flex flex-col overflow-hidden min-h-0">
            {breadcrumbs.length > 1 && (
              <div className="flex items-center space-x-2 text-sm text-muted-foreground mb-2 overflow-x-auto flex-shrink-0">
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

            <div className="flex-1 min-h-0 border border-border/60 rounded-lg p-6">
              {visualizationType === "treemap" ? (
                <TreeMapChart
                  data={currentViewNode}
                  rampStops={rampStops}
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

      <footer className="border-t border-border/40 px-6 py-3 flex-shrink-0 bg-muted/30">
        <div className="flex items-center justify-between text-xs text-muted-foreground">
          {/* Left: brand + live stats */}
          <div className="flex items-center space-x-3">
            <span className="font-medium text-foreground">diskviz</span>
            {summary && currentViewNode && (
              <>
                <span className="text-border">·</span>
                <span className="tabular-nums">{formatFileSize(currentViewNode.size)}</span>
                <span className="text-border">·</span>
                <span className="tabular-nums">{currentViewNode.fileCount.toLocaleString()} files</span>
                <span className="text-border">·</span>
                <span className="tabular-nums">{currentViewNode.dirCount.toLocaleString()} dirs</span>
                <span className="text-border">·</span>
                <span className="tabular-nums">{formatDuration(summary.scanDurationMs)}</span>
              </>
            )}
          </div>

          {/* Right: controls */}
          <div className="flex items-center space-x-3">
            <AccentPicker
              accent={accent}
              setAccent={setAccent as (a: AccentColor) => void}
              accentColor={accentColor}
              resolvedFlavor={resolvedFlavor}
            />

            <span className="text-border">·</span>

            <ThemePicker
              theme={theme}
              setTheme={setTheme as (t: ThemeSetting) => void}
            />

            <span className="text-border">·</span>

            {/* View toggle */}
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
              <ToggleGroupItem value="treemap" aria-label="TreeMap view" className="h-6 w-6 p-0">
                <BarChart3 className="h-3 w-3" />
              </ToggleGroupItem>
              <ToggleGroupItem value="sunburst" aria-label="Sunburst view" className="h-6 w-6 p-0">
                <Target className="h-3 w-3" />
              </ToggleGroupItem>
            </ToggleGroup>

            <span className="text-border">·</span>

            <button
              type="button"
              onClick={() => setShowShortcuts((s) => !s)}
              className="flex items-center gap-1.5 border border-border/60 rounded px-2 py-0.5 hover:bg-muted/60 hover:text-foreground hover:border-border transition-colors"
              title="Keyboard shortcuts (Cmd+?)"
            >
              <KeyboardIcon className="w-3 h-3" />
              <span>Shortcuts</span>
            </button>

            <button
              type="button"
              onClick={() => setShowNotices(true)}
              className="flex items-center gap-1.5 border border-border/60 rounded px-2 py-0.5 hover:bg-muted/60 hover:text-foreground hover:border-border transition-colors"
              title="Open-source notices"
            >
              <InfoIcon className="w-3 h-3" />
              <span>Notices</span>
            </button>
          </div>
        </div>
      </footer>

      {showNotices && <NoticesModal onClose={() => setShowNotices(false)} />}

      {showShortcuts && (
        <div
          className="fixed inset-0 bg-black/50 flex items-center justify-center z-[10000]"
          onClick={() => setShowShortcuts(false)}
        >
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="shortcuts-title"
            className="bg-background border border-border rounded-lg shadow-xl p-6 max-w-md w-full mx-4"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between mb-4">
              <h3 id="shortcuts-title" className="text-lg font-semibold">Keyboard Shortcuts</h3>
              <button
                ref={closeButtonRef}
                type="button"
                aria-label="Close shortcuts"
                onClick={() => setShowShortcuts(false)}
                className="text-muted-foreground hover:text-foreground transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring rounded"
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
                <span className="text-muted-foreground">Go back</span>
                <kbd className="bg-muted px-2 py-1 rounded text-xs font-mono">Backspace</kbd>
              </div>
              <div className="flex justify-between items-center">
                <span className="text-muted-foreground">Go back (mobile)</span>
                <span className="text-xs text-muted-foreground">Swipe right</span>
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
