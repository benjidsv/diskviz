import type React from "react";
import { ExternalLinkIcon } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import Modal from "@/components/ui/modal";

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

const NoticesModal: React.FC<NoticesModalProps> = ({ onClose }) => (
  <Modal titleId="notices-title" title="Open-source notices" closeLabel="Close notices" onClose={onClose}>
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
  </Modal>
);

export default NoticesModal;
