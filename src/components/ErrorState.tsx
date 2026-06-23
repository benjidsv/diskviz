import type React from "react";
import { AlertTriangleIcon } from "lucide-react";
import { Button } from "@/components/ui/button";

interface ErrorStateProps {
  message: string;
  /** Present for retryable failures (e.g. a scan); omitted for navigation errors. */
  onRetry?: () => void;
  onDismiss: () => void;
}

export const ErrorState: React.FC<ErrorStateProps> = ({ message, onRetry, onDismiss }) => (
  <div className="flex flex-col items-center justify-center flex-1 space-y-4 px-6 text-center">
    <div className="bg-destructive/10 p-6 rounded-full">
      <AlertTriangleIcon className="h-12 w-12 text-destructive" />
    </div>
    <div className="space-y-2 max-w-md">
      <h3 className="text-xl font-semibold text-foreground">Couldn’t read that location</h3>
      <p className="text-sm text-muted-foreground break-words">{message}</p>
    </div>
    <div className="flex items-center gap-2">
      {onRetry && <Button onClick={onRetry}>Try again</Button>}
      <Button variant="outline" onClick={onDismiss}>
        Dismiss
      </Button>
    </div>
  </div>
);
