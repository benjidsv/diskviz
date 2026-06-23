import * as React from "react";
import { cn } from "@/lib/utils";
import { Popover, PopoverClose, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { ACCENT_COLORS, ALL_ACCENTS } from "@/hooks/useThemeSettings";
import type { AccentColor, ThemeFlavor } from "@/hooks/useThemeSettings";

const ACCENT_LABELS: Record<AccentColor, string> = {
  rosewater: "Rosewater", flamingo: "Flamingo", pink:     "Pink",
  mauve:     "Mauve",     red:      "Red",      maroon:   "Maroon",
  peach:     "Peach",     yellow:   "Yellow",   green:    "Green",
  teal:      "Teal",      sky:      "Sky",       sapphire: "Sapphire",
  blue:      "Blue",      lavender: "Lavender",
};

interface AccentPickerProps {
  accent: AccentColor;
  setAccent: (a: AccentColor) => void;
  accentColor: string;
  resolvedFlavor: ThemeFlavor;
}

export const AccentPicker: React.FC<AccentPickerProps> = ({
  accent,
  setAccent,
  accentColor,
  resolvedFlavor,
}) => {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={`Accent color: ${ACCENT_LABELS[accent]}`}
          aria-haspopup="dialog"
          className="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring rounded"
        >
          <span
            className="w-3 h-3 rounded-full flex-shrink-0 ring-1 ring-border/50"
            style={{ backgroundColor: accentColor }}
          />
          <span>{ACCENT_LABELS[accent]}</span>
        </button>
      </PopoverTrigger>
      <PopoverContent side="top" align="end" className="w-auto">
        <div className="grid grid-cols-7 gap-1">
          {ALL_ACCENTS.map((a) => {
            const color = ACCENT_COLORS[resolvedFlavor][a];
            const isSelected = a === accent;
            return (
              <PopoverClose asChild key={a}>
                <button
                  type="button"
                  aria-label={ACCENT_LABELS[a]}
                  aria-pressed={isSelected}
                  onClick={() => setAccent(a)}
                  className={cn(
                    "w-5 h-5 rounded-full transition-transform hover:scale-110 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-popover",
                    isSelected && "ring-2 ring-ring ring-offset-1 ring-offset-popover",
                  )}
                  style={{ backgroundColor: color }}
                  title={ACCENT_LABELS[a]}
                />
              </PopoverClose>
            );
          })}
        </div>
      </PopoverContent>
    </Popover>
  );
};
