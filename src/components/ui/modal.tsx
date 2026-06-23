import type React from "react";
import { useEffect, useRef } from "react";
import { XIcon } from "lucide-react";

interface ModalProps {
  /** Dialog id used by aria-labelledby — must match the h3's id */
  titleId: string;
  title: string;
  /** aria-label for the close button */
  closeLabel?: string;
  onClose: () => void;
  children: React.ReactNode;
}

/**
 * Shared modal shell used by ShortcutsModal and NoticesModal.
 *
 * - bg-black/50 scrim (intentionally theme-independent per CLAUDE.md)
 * - Centered max-w-md card, shadow-xl
 * - Click-outside to close, Escape to close
 * - Autofocuses the close button on open
 * - role="dialog" aria-modal="true"
 */
const Modal: React.FC<ModalProps> = ({ titleId, title, closeLabel = "Close", onClose, children }) => {
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
        aria-labelledby={titleId}
        className="bg-background border border-border rounded-lg shadow-xl p-6 max-w-md w-full mx-4"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between mb-5">
          <h3 id={titleId} className="text-lg font-semibold">{title}</h3>
          <button
            ref={closeButtonRef}
            type="button"
            aria-label={closeLabel}
            onClick={onClose}
            className="text-muted-foreground hover:text-foreground transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring rounded"
          >
            <XIcon className="w-5 h-5" />
          </button>
        </div>
        {children}
      </div>
    </div>
  );
};

export default Modal;
