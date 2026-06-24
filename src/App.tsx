import { BarChart3, HardDriveIcon, History, InfoIcon, KeyboardIcon, Ruler, Target } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { Button } from "@/components/ui/button";
import { Dot } from "@/components/ui/dot";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { useVisualizationSettings } from "@/hooks/useVisualizationSettings";
import { useThemeSettings, VIZ_RAMP_BASE } from "@/hooks/useThemeSettings";
import type { ThemeSetting, AccentColor } from "@/hooks/useThemeSettings";
import {
  cancelScan,
  deleteNode,
  getSubtree,
  onScanProgress,
  pickDirectory,
  scanDirectory,
  validatePath,
} from "@/lib/api";
import { cn } from "@/lib/utils";
import { isOtherNode } from "@/hooks/useTreeMapData";
import type { FileNode, ScanProgress as Progress, ScanSummary } from "@/types";
import SunburstChart from "@/components/charts/SunburstChart";
import TreeMapChart from "@/components/charts/TreeMapChart";
import { ColorScaleLegend } from "@/components/charts/ColorScaleLegend";
import { DetailReadout } from "@/components/charts/DetailReadout";
import ScanProgress from "@/components/ScanProgress";
import NoticesModal from "@/components/NoticesModal";
import { ErrorState } from "@/components/ErrorState";
import { TopBar } from "@/components/TopBar";
import Modal from "@/components/ui/modal";
import { AccentPicker } from "@/components/AccentPicker";
import { ThemePicker } from "@/components/ThemePicker";
import { showBreadcrumbContextMenu } from "@/hooks/useNativeContextMenu";

function App() {
  const [summary, setSummary] = useState<ScanSummary | null>(null);
  const [isScanning, setIsScanning] = useState(false);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [currentViewNode, setCurrentViewNode] = useState<FileNode | null>(null);
  const [breadcrumbs, setBreadcrumbs] = useState<FileNode[]>([]);
  const [forwardStack, setForwardStack] = useState<FileNode[]>([]);
  const [selectedNode, setSelectedNode] = useState<FileNode | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isDragOver, setIsDragOver] = useState(false);
  const [showShortcuts, setShowShortcuts] = useState(false);
  const [showNotices, setShowNotices] = useState(false);
  const {
    visualizationType,
    setVisualizationType,
    colorMode,
    setColorMode,
    ageThresholdDays,
    setAgeThresholdDays,
  } = useVisualizationSettings();
  const { theme, setTheme, accent, setAccent, resolvedFlavor, accentColor, ageRampStops } =
    useThemeSettings();

  const scannedPath = useRef<string>("");

  // Full 5-stop ramp: 4 static base stops + current accent as ramp-5
  const rampStops = [...VIZ_RAMP_BASE[resolvedFlavor], accentColor] as string[];

  useEffect(() => {
    const unlisten = onScanProgress(setProgress);
    return () => {
      unlisten.then((fn) => fn()).catch(() => {});
    };
  }, []);

  // Suppress the default WebView context menu everywhere in production
  // so native right-clicks are handled exclusively by our Tauri menus.
  // Kept off in dev mode so Inspect Element / DevTools remain accessible.
  useEffect(() => {
    if (import.meta.env.DEV) return;
    const suppress = (e: MouseEvent) => e.preventDefault();
    document.addEventListener("contextmenu", suppress);
    return () => document.removeEventListener("contextmenu", suppress);
  }, []);

  const handleScanDirectory = useCallback(async (path: string) => {
    if (!path) return;
    setError(null);
    setProgress(null);
    setIsScanning(true);
    scannedPath.current = path;

    try {
      const result = await scanDirectory(path);
      const root = await getSubtree(result.rootId);
      setSummary(result);
      setCurrentViewNode(root);
      setBreadcrumbs([root]);
      setForwardStack([]);
      setSelectedNode(null);
    } catch (err) {
      // The backend reports a user cancellation as the "cancelled" sentinel —
      // keep the prior view intact rather than surfacing an error.
      const msg = String(err);
      if (!msg.toLowerCase().includes("cancelled")) {
        setError(msg);
      }
    } finally {
      setIsScanning(false);
    }
  }, []);

  // Pick a folder and analyze it in one step (no separate "Analyze" action).
  const handleFolderPicker = useCallback(async () => {
    try {
      const selected = await pickDirectory();
      if (selected) void handleScanDirectory(selected);
    } catch (e) {
      console.error("Failed to open directory dialog:", e);
    }
  }, [handleScanDirectory]);

  const handleCancelScan = useCallback(() => {
    void cancelScan();
  }, []);

  // Load a directory view. With offset > 0 it's an "Other" page (items past the
  // current batch); we tag it as synthetic so navigation can re-page it and the
  // breadcrumb can show it as "Other" without colliding with the real node id.
  const loadView = useCallback(async (baseId: string, offset: number): Promise<FileNode> => {
    const fresh = await getSubtree(baseId, 3, 100, offset);
    if (offset <= 0) return fresh;
    return {
      ...fresh,
      id: `${baseId}::other::${offset}`,
      name: "Other",
      overflowBaseId: baseId,
      overflowOffset: offset,
    };
  }, []);

  // Re-fetch the view a navigation entry represents (handles "Other" pages too).
  const refreshView = useCallback(
    (node: FileNode) => loadView(node.overflowBaseId ?? node.id, node.overflowOffset ?? 0),
    [loadView],
  );

  const handleNodeDoubleClick = useCallback(
    async (node: FileNode) => {
      if (node.type !== "directory") return;
      const isOverflow = isOtherNode(node);
      const baseId = isOverflow ? node.overflowBaseId : node.id;
      const offset = isOverflow ? node.overflowOffset ?? 0 : 0;
      if (!baseId) return;
      try {
        const fresh = await loadView(baseId, offset);
        // Real directories that turn out empty aren't worth drilling into;
        // an "Other" page always has children by construction.
        if (!isOverflow && (!fresh.children || fresh.children.length === 0)) return;
        setForwardStack([]);
        setSelectedNode(null);
        setCurrentViewNode(fresh);
        setBreadcrumbs((prev) => [...prev, fresh]);
      } catch (err) {
        setError(String(err));
      }
    },
    [loadView],
  );

  const handleBreadcrumbClick = useCallback(
    async (index: number) => {
      const target = breadcrumbs[index];
      if (!target) return;
      try {
        const fresh = await refreshView(target);
        setForwardStack([]);
        setSelectedNode(null);
        setBreadcrumbs((prev) => prev.slice(0, index + 1));
        setCurrentViewNode(fresh);
      } catch (err) {
        setError(String(err));
      }
    },
    [breadcrumbs, refreshView],
  );

  const handleNodeDeleted = useCallback(
    async (node: FileNode) => {
      if (!currentViewNode) return;
      try {
        const newSummary = await deleteNode(node.id);
        setSummary(newSummary);
        const fresh = await refreshView(currentViewNode);
        setSelectedNode(null);
        setCurrentViewNode(fresh);
      } catch (error) {
        console.error("Incremental delete failed, falling back to full rescan:", error);
        void handleScanDirectory(scannedPath.current);
      }
    },
    [currentViewNode, handleScanDirectory, refreshView],
  );

  const handleGoBack = useCallback(async () => {
    if (breadcrumbs.length <= 1) return;
    const currentTip = breadcrumbs[breadcrumbs.length - 1];
    const target = breadcrumbs[breadcrumbs.length - 2];
    try {
      const fresh = await refreshView(target);
      setForwardStack((prev) => [currentTip, ...prev]);
      setBreadcrumbs((prev) => prev.slice(0, -1));
      setSelectedNode(null);
      setCurrentViewNode(fresh);
    } catch (err) {
      setError(String(err));
    }
  }, [breadcrumbs, refreshView]);

  const handleGoForward = useCallback(async () => {
    if (forwardStack.length === 0) return;
    const [next, ...rest] = forwardStack;
    try {
      const fresh = await refreshView(next);
      setBreadcrumbs((prev) => [...prev, fresh]);
      setSelectedNode(null);
      setCurrentViewNode(fresh);
      setForwardStack(rest);
    } catch (err) {
      setError(String(err));
      setForwardStack([]);
    }
  }, [forwardStack, refreshView]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      // Don't intercept shortcuts when the user is typing in an input/textarea
      const tag = (event.target as HTMLElement)?.tagName;
      const isTyping = tag === "INPUT" || tag === "TEXTAREA";

      if (event.key === "Escape" && !isTyping) {
        if (showShortcuts) {
          setShowShortcuts(false);
        } else {
          setSelectedNode(null);
        }
        return;
      }

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
          case "z":
            if (!isTyping && !showShortcuts) {
              event.preventDefault();
              void handleGoBack();
            }
            break;
          case "Z": // Shift+Z
            if (!isTyping && !showShortcuts) {
              event.preventDefault();
              void handleGoForward();
            }
            break;
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleFolderPicker, handleGoBack, handleGoForward, showShortcuts]);

  // ── Native menu event listeners ────────────────────────────────────────────
  // Each event mirrors the corresponding in-app UI control.
  useEffect(() => {
    const unlisteners = [
      listen("menu-open-folder", () => { void handleFolderPicker(); }),
      listen("menu-show-shortcuts", () => { setShowShortcuts(true); }),
      listen("menu-show-notices", () => { setShowNotices(true); }),
      listen<string>("menu-set-theme", (e) => {
        const v = e.payload as ThemeSetting;
        setTheme(v);
      }),
      listen<string>("menu-set-accent", (e) => {
        const v = e.payload as AccentColor;
        setAccent(v);
      }),
      listen<string>("menu-set-visualization", (e) => {
        const v = e.payload;
        if (v === "treemap" || v === "sunburst") setVisualizationType(v);
      }),
    ];
    return () => {
      unlisteners.forEach((p) => p.then((fn) => fn()).catch(() => {}));
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Drag-and-drop a folder onto the window to analyze it ────────────────────
  const isScanningRef = useRef(isScanning);
  isScanningRef.current = isScanning;
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const p = event.payload;
        if (p.type === "enter" || p.type === "over") {
          setIsDragOver(true);
        } else if (p.type === "leave") {
          setIsDragOver(false);
        } else if (p.type === "drop") {
          setIsDragOver(false);
          if (isScanningRef.current) return;
          const path = p.paths?.[0];
          if (!path) return;
          validatePath(path)
            .then((ok) => { if (ok) void handleScanDirectory(path); })
            .catch(() => {});
        }
      })
      .then((fn) => { unlisten = fn; })
      .catch(() => {});
    return () => unlisten?.();
  }, [handleScanDirectory]);

  // Trackpad swipe gesture using wheel events
  const swipeStateRef = useRef({ accumulatedBack: 0, accumulatedForward: 0, cooldownUntil: 0 });
  const swipeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const handleWheel = useCallback(
    (e: WheelEvent) => {
      // Block all native scroll/pan — this is a canvas-like app with no scrollable regions
      e.preventDefault();

      const now = Date.now();
      const state = swipeStateRef.current;

      if (now < state.cooldownUntil) return;

      // Must be primarily horizontal; reset both accumulators on vertical/diagonal
      if (Math.abs(e.deltaX) <= Math.abs(e.deltaY)) {
        state.accumulatedBack = 0;
        state.accumulatedForward = 0;
        if (swipeTimerRef.current) clearTimeout(swipeTimerRef.current);
        return;
      }

      if (swipeTimerRef.current) clearTimeout(swipeTimerRef.current);
      swipeTimerRef.current = setTimeout(() => {
        state.accumulatedBack = 0;
        state.accumulatedForward = 0;
      }, 300);

      if (e.deltaX < 0) {
        // Right swipe (fingers moving right, negative deltaX) → go back
        state.accumulatedBack += Math.abs(e.deltaX);
        state.accumulatedForward = 0;
        if (state.accumulatedBack > 150) {
          handleGoBack();
          state.accumulatedBack = 0;
          state.cooldownUntil = now + 800;
        }
      } else {
        // Left swipe (fingers moving left, positive deltaX) → go forward
        state.accumulatedForward += e.deltaX;
        state.accumulatedBack = 0;
        if (state.accumulatedForward > 150) {
          handleGoForward();
          state.accumulatedForward = 0;
          state.cooldownUntil = now + 800;
        }
      }
    },
    [handleGoBack, handleGoForward],
  );

  useEffect(() => {
    // Must be non-passive to allow preventDefault()
    window.addEventListener("wheel", handleWheel as EventListener, { passive: false });
    return () => {
      window.removeEventListener("wheel", handleWheel as EventListener);
      if (swipeTimerRef.current) clearTimeout(swipeTimerRef.current);
    };
  }, [handleWheel]);

  const hasScan = !!(summary && currentViewNode);
  const readoutNode = selectedNode ?? currentViewNode;

  return (
    <div className="h-screen bg-background overflow-hidden flex flex-col">
      {hasScan && (
        <TopBar
          breadcrumbs={breadcrumbs}
          onBreadcrumbClick={handleBreadcrumbClick}
          onBreadcrumbContextMenu={(crumb) => void showBreadcrumbContextMenu(crumb)}
          canGoBack={breadcrumbs.length > 1}
          canGoForward={forwardStack.length > 0}
          onBack={() => void handleGoBack()}
          onForward={() => void handleGoForward()}
          onOpen={() => void handleFolderPicker()}
          currentViewNode={currentViewNode}
          summary={summary}
        />
      )}

      <main className="flex-1 flex flex-col overflow-hidden min-h-0 px-8 pt-5 pb-3">
        {isScanning ? (
          <div className="flex items-center justify-center flex-1">
            <ScanProgress progress={progress} rootPath={scannedPath.current} onCancel={handleCancelScan} />
          </div>
        ) : error ? (
          <ErrorState
            message={error}
            onRetry={scannedPath.current ? () => void handleScanDirectory(scannedPath.current) : undefined}
            onDismiss={() => setError(null)}
          />
        ) : hasScan ? (
          <div className="flex-1 min-h-0 flex flex-col">
            <div className="flex-1 min-h-0">
              {visualizationType === "treemap" ? (
                <TreeMapChart
                  data={currentViewNode}
                  rampStops={rampStops}
                  colorMode={colorMode}
                  ageRampStops={ageRampStops}
                  ageThresholdDays={ageThresholdDays}
                  selectedId={selectedNode?.id}
                  onNodeSelect={setSelectedNode}
                  onNodeDoubleClick={handleNodeDoubleClick}
                  onNodeDeleted={handleNodeDeleted}
                />
              ) : (
                <SunburstChart
                  data={currentViewNode}
                  colorMode={colorMode}
                  ageRampStops={ageRampStops}
                  ageThresholdDays={ageThresholdDays}
                  selectedId={selectedNode?.id}
                  onNodeSelect={setSelectedNode}
                  onNodeDoubleClick={handleNodeDoubleClick}
                  onNodeDeleted={handleNodeDeleted}
                />
              )}
            </div>

            {/* Viz footer strip: detail readout + (treemap) color legend */}
            <div className="flex items-center justify-between gap-4 pt-3 mt-3 border-t border-border/60">
              <DetailReadout
                node={readoutNode}
                parentSize={currentViewNode.size}
                isSelection={!!selectedNode}
                ageThresholdDays={ageThresholdDays}
              />
              {(visualizationType === "treemap" || colorMode === "activeness") && (
                <ColorScaleLegend
                  rampStops={rampStops}
                  colorMode={colorMode}
                  ageRampStops={ageRampStops}
                />
              )}
            </div>
          </div>
        ) : (
          <div
            className={cn(
              "flex flex-col items-center justify-center flex-1 space-y-5 rounded-lg transition-colors",
              isDragOver && "bg-primary/5 outline-dashed outline-2 outline-primary/40",
            )}
          >
            <div className={cn("p-6 rounded-full transition-colors", isDragOver ? "bg-primary/15" : "bg-muted/30")}>
              <HardDriveIcon className={cn("h-12 w-12", isDragOver ? "text-primary" : "text-muted-foreground")} />
            </div>
            <div className="text-center space-y-2">
              <h3 className="text-xl font-semibold text-foreground">
                {isDragOver ? "Drop to analyze" : "Choose a directory"}
              </h3>
              <p className="text-muted-foreground text-sm">
                Drag a folder here, or browse to visualize what&apos;s using your space
              </p>
            </div>
            <Button onClick={handleFolderPicker}>Browse…</Button>
            <p className="micro-label text-muted-foreground">Open folder · ⌘O</p>
          </div>
        )}
      </main>

      <footer className="border-t border-border/60 px-6 py-2 flex-shrink-0 bg-muted/30">
        <div className="flex items-center justify-between text-xs text-muted-foreground">
          {/* Left: a quiet interaction hint */}
          <span className="micro-label hidden sm:inline">Double-click to drill in · right-click for actions</span>

          {/* Right: settings */}
          <div className="flex items-center gap-3">
            <AccentPicker
              accent={accent}
              setAccent={setAccent as (a: AccentColor) => void}
              accentColor={accentColor}
              resolvedFlavor={resolvedFlavor}
            />

            <Dot />

            <ThemePicker
              theme={theme}
              setTheme={setTheme as (t: ThemeSetting) => void}
            />

            <Dot />

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

            <Dot />

            <ToggleGroup
              type="single"
              value={colorMode}
              onValueChange={(value) => {
                if (value === "size" || value === "activeness") setColorMode(value);
              }}
              variant="outline"
              size="sm"
            >
              <ToggleGroupItem value="size" aria-label="Color by size" className="h-6 w-6 p-0">
                <Ruler className="h-3 w-3" />
              </ToggleGroupItem>
              <ToggleGroupItem value="activeness" aria-label="Color by activeness" className="h-6 w-6 p-0">
                <History className="h-3 w-3" />
              </ToggleGroupItem>
            </ToggleGroup>

            {colorMode === "activeness" && (
              <Popover>
                <PopoverTrigger asChild>
                  <Button variant="outline" size="sm" className="gap-1.5" title="Activeness threshold">
                    <History className="w-3.5 h-3.5" />
                    {(ageThresholdDays / 365).toFixed(ageThresholdDays % 365 === 0 ? 0 : 1)}y
                  </Button>
                </PopoverTrigger>
                <PopoverContent className="w-56 space-y-2">
                  <span className="micro-label text-muted-foreground">Dormant after (years)</span>
                  <Input
                    type="number"
                    min="0.5"
                    max="10"
                    step="0.5"
                    value={(ageThresholdDays / 365).toString()}
                    onChange={(e) => {
                      const years = Number(e.target.value);
                      if (years > 0) setAgeThresholdDays(Math.round(years * 365));
                    }}
                    className="w-full"
                  />
                  <p className="text-xs text-muted-foreground">
                    Folders whose median file is this old read fully “old” (red).
                  </p>
                </PopoverContent>
              </Popover>
            )}

            <Dot />

            <Button
              variant="outline"
              size="sm"
              onClick={() => setShowShortcuts((s) => !s)}
              className="gap-1.5"
              title="Keyboard shortcuts (⌘?)"
            >
              <KeyboardIcon className="w-3.5 h-3.5" />
              Shortcuts
            </Button>

            <Button
              variant="outline"
              size="sm"
              onClick={() => setShowNotices(true)}
              className="gap-1.5"
              title="Open-source notices"
            >
              <InfoIcon className="w-3.5 h-3.5" />
              Notices
            </Button>
          </div>
        </div>
      </footer>

      {showNotices && <NoticesModal onClose={() => setShowNotices(false)} />}

      {showShortcuts && (
        <Modal
          titleId="shortcuts-title"
          title="Keyboard Shortcuts"
          closeLabel="Close shortcuts"
          onClose={() => setShowShortcuts(false)}
        >
          <div className="space-y-3 text-sm">
            <ShortcutRow label="Open folder" keys={["⌘O"]} />
            <ShortcutRow label="Drill into folder" keys={["Double-click"]} />
            <ShortcutRow label="Select an item" keys={["Click"]} />
            <ShortcutRow label="Deselect / dismiss" keys={["Esc"]} />
            <ShortcutRow label="Navigate back / forward" keys={["⌘Z", "⌘⇧Z"]} />
            <ShortcutRow label="Swipe to navigate back / forward" keys={["← →"]} />
            <ShortcutRow label="Jump to an ancestor" keys={["Click breadcrumb"]} />
            <ShortcutRow label="Show shortcuts" keys={["⌘?"]} />
          </div>
        </Modal>
      )}
    </div>
  );
}

function ShortcutRow({ label, keys }: { label: string; keys: string[] }) {
  return (
    <div className="flex justify-between items-center">
      <span className="text-muted-foreground">{label}</span>
      <div className="flex items-center gap-1">
        {keys.map((k, i) => (
          <span key={k} className="flex items-center gap-1">
            {i > 0 && <span className="text-muted-foreground text-xs">/</span>}
            <kbd className="bg-muted px-2 py-1 rounded text-xs font-mono">{k}</kbd>
          </span>
        ))}
      </div>
    </div>
  );
}

export default App;
