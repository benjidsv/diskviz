import type React from "react";
import { useEffect, useRef, useState } from "react";
import { HardDriveIcon } from "lucide-react";
import type { ScanProgress as Progress } from "@/types";
import { formatFileSize, formatNumber } from "@/utils/formatters";
import { Button } from "@/components/ui/button";

interface ScanProgressProps {
  progress: Progress | null;
  rootPath: string;
  onCancel: () => void;
}

const Stat: React.FC<{ label: string; value: string }> = ({ label, value }) => (
  <div className="rounded-lg border border-border/60 p-3">
    <div className="text-base font-semibold font-mono tabular-nums">{value}</div>
    <div className="text-xs text-muted-foreground">{label}</div>
  </div>
);

const ScanProgress: React.FC<ScanProgressProps> = ({ progress, rootPath, onCancel }) => {
  const pct = Math.max(0, Math.min(100, progress?.percent ?? 0));
  const startedAt = useRef(Date.now());
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    const id = setInterval(() => setElapsed(Math.floor((Date.now() - startedAt.current) / 1000)), 500);
    return () => clearInterval(id);
  }, []);

  return (
    <div className="w-full max-w-xl space-y-7">
      <div className="flex items-center space-x-3">
        <div className="bg-muted/40 p-3 rounded-full">
          <HardDriveIcon className="h-6 w-6 text-muted-foreground" />
        </div>
        <div className="min-w-0">
          <h3 className="text-lg font-medium text-foreground">Analyzing directory…</h3>
          <p className="text-xs text-muted-foreground truncate">
            {progress?.currentPath || rootPath}
          </p>
        </div>
      </div>

      <div className="space-y-2">
        <div className="flex justify-between text-sm">
          <span className="text-muted-foreground">Progress</span>
          <span className="font-mono tabular-nums text-muted-foreground">
            {elapsed}s · {pct.toFixed(0)}%
          </span>
        </div>
        <div
          className="h-2.5 w-full overflow-hidden rounded-full bg-muted"
          role="progressbar"
          aria-label="Scan progress"
          aria-valuenow={Math.round(pct)}
          aria-valuemin={0}
          aria-valuemax={100}
        >
          <div
            className="h-full rounded-full bg-primary transition-[width] duration-200 ease-out"
            style={{ width: `${pct}%` }}
          />
        </div>
      </div>

      <div className="grid grid-cols-3 gap-3 text-center">
        <Stat label="Files" value={formatNumber(progress?.filesScanned ?? 0)} />
        <Stat label="Folders" value={formatNumber(progress?.directoriesScanned ?? 0)} />
        <Stat label="Scanned" value={formatFileSize(progress?.bytesScanned ?? 0)} />
      </div>

      <div className="flex justify-center">
        <Button variant="outline" size="sm" onClick={onCancel}>
          Cancel scan
        </Button>
      </div>
    </div>
  );
};

export default ScanProgress;
