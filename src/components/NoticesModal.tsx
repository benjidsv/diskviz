import type React from "react";
import { ExternalLinkIcon, XIcon } from "lucide-react";
import { useEffect, useRef } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

interface NoticesModalProps {
  onClose: () => void;
}

const Link: React.FC<{ href: string; children: React.ReactNode }> = ({ href, children }) => (
  <button
    type="button"
    onClick={() => openUrl(href)}
    className="inline-flex items-center gap-0.5 text-primary underline underline-offset-2 hover:opacity-80 transition-opacity"
  >
    {children}
    <ExternalLinkIcon className="w-3 h-3 shrink-0" />
  </button>
);

const NoticesModal: React.FC<NoticesModalProps> = ({ onClose }) => {
  const closeButtonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    closeButtonRef.current?.focus();
  }, []);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-[10000]"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="notices-title"
        className="bg-background border border-border rounded-lg shadow-xl p-6 max-w-md w-full mx-4"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-5">
          <h3 id="notices-title" className="text-lg font-semibold">Open-source notices</h3>
          <button
            ref={closeButtonRef}
            type="button"
            aria-label="Close notices"
            onClick={onClose}
            className="text-muted-foreground hover:text-foreground transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring rounded"
          >
            <XIcon className="w-5 h-5" />
          </button>
        </div>

        <div className="space-y-5 text-sm">
          <section>
            <div className="flex items-center justify-between mb-1.5">
              <Link href="https://github.com/kiwamizamurai/vizdisk">vizdisk</Link>
              <span className="text-xs text-muted-foreground">MIT · kiwamizamurai</span>
            </div>
            <p className="text-muted-foreground text-xs leading-relaxed">
              The treemap and sunburst chart components, shadcn/ui primitives, and overall UI
              layout are adapted from vizdisk. The disk scanning backend (Rust + jwalk) and data
              flow are original to diskviz.
            </p>
          </section>

          <div className="border-t border-border/60" />

          <section>
            <div className="flex items-center justify-between mb-1.5">
              <Link href="https://catppuccin.com">Catppuccin</Link>
              <span className="text-xs text-muted-foreground">MIT · Catppuccin</span>
            </div>
            <p className="text-muted-foreground text-xs leading-relaxed">
              All four themes — Latte, Frappé, Macchiato, and Mocha — use the Catppuccin color
              palette.
            </p>
          </section>
        </div>
      </div>
    </div>
  );
};

export default NoticesModal;
