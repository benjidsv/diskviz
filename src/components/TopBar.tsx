import type React from "react";
import { Fragment } from "react";
import { ChevronLeftIcon, ChevronRightIcon, FolderOpenIcon } from "lucide-react";
import { cn } from "@/lib/utils";
import { Dot } from "@/components/ui/dot";
import { formatDuration, formatFileSize } from "@/utils/formatters";
import type { FileNode, ScanSummary } from "@/types";

interface TopBarProps {
  breadcrumbs: FileNode[];
  onBreadcrumbClick: (index: number) => void;
  onBreadcrumbContextMenu: (crumb: FileNode) => void;
  canGoBack: boolean;
  canGoForward: boolean;
  onBack: () => void;
  onForward: () => void;
  onOpen: () => void;
  currentViewNode: FileNode | null;
  summary: ScanSummary | null;
}

const NavButton: React.FC<{
  label: string;
  disabled: boolean;
  onClick: () => void;
  children: React.ReactNode;
}> = ({ label, disabled, onClick, children }) => (
  <button
    type="button"
    aria-label={label}
    title={label}
    disabled={disabled}
    onClick={onClick}
    className="flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground disabled:pointer-events-none disabled:opacity-30 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
  >
    {children}
  </button>
);

/**
 * The instrument HUD: brand · back/forward · breadcrumb trail on the left,
 * live stats + Open on the right. Owns "where am I" via a single breadcrumb.
 */
export const TopBar: React.FC<TopBarProps> = ({
  breadcrumbs,
  onBreadcrumbClick,
  onBreadcrumbContextMenu,
  canGoBack,
  canGoForward,
  onBack,
  onForward,
  onOpen,
  currentViewNode,
  summary,
}) => {
  return (
    <header className="flex items-center gap-3 border-b border-border/60 bg-muted/30 px-4 py-2 flex-shrink-0">
      {/* Brand */}
      <span className="micro-label font-semibold text-foreground tracking-[0.18em] shrink-0 select-none">
        DISKVIZ
      </span>

      {/* Back / forward */}
      <div className="flex items-center gap-0.5 shrink-0">
        <NavButton label="Go back (⌘Z)" disabled={!canGoBack} onClick={onBack}>
          <ChevronLeftIcon className="h-4 w-4" />
        </NavButton>
        <NavButton label="Go forward (⌘⇧Z)" disabled={!canGoForward} onClick={onForward}>
          <ChevronRightIcon className="h-4 w-4" />
        </NavButton>
      </div>

      {/* Breadcrumb trail — the single source of truth for location */}
      <nav
        aria-label="Breadcrumb"
        className="flex items-center text-xs font-mono text-muted-foreground overflow-x-auto min-w-0 flex-1 select-none"
      >
        {breadcrumbs.map((crumb, index) => {
          const isLast = index === breadcrumbs.length - 1;
          const isRoot = index === 0;
          // Avoid the "/ / folder" doubling when the scan root is the volume root.
          const skipSeparator = isRoot && crumb.name === "/";
          return (
            <Fragment key={crumb.id}>
              <button
                type="button"
                onClick={isLast ? undefined : () => onBreadcrumbClick(index)}
                onContextMenu={(e) => {
                  e.preventDefault();
                  onBreadcrumbContextMenu(crumb);
                }}
                className={cn(
                  "px-1.5 py-1 whitespace-nowrap rounded-sm transition-colors",
                  isRoot && "text-primary font-medium",
                  isLast
                    ? "text-foreground font-medium cursor-default"
                    : "hover:text-foreground cursor-pointer",
                  isRoot && !isLast && "hover:text-primary/80",
                )}
              >
                {crumb.name}
              </button>
              {!isLast && !skipSeparator && (
                <span className="text-muted-foreground/30">/</span>
              )}
            </Fragment>
          );
        })}
      </nav>

      {/* Live stats */}
      {currentViewNode && summary && (
        <div className="readout flex items-center gap-2 text-xs text-muted-foreground shrink-0">
          <span className="tabular-nums">{formatFileSize(currentViewNode.size)}</span>
          <Dot />
          <span className="tabular-nums">{currentViewNode.fileCount.toLocaleString()} files</span>
          <Dot />
          <span className="tabular-nums">{currentViewNode.dirCount.toLocaleString()} folders</span>
          <Dot />
          <span className="tabular-nums">{formatDuration(summary.scanDurationMs)}</span>
        </div>
      )}

      {/* Open a new directory */}
      <button
        type="button"
        onClick={onOpen}
        title="Open folder (⌘O)"
        className="flex items-center gap-1.5 rounded-md border border-border/60 px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground hover:border-border focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring shrink-0"
      >
        <FolderOpenIcon className="h-3.5 w-3.5" />
        <span>Open</span>
      </button>
    </header>
  );
};
