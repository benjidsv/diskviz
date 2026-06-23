import * as React from "react";
import { CheckIcon } from "lucide-react";
import { cn } from "@/lib/utils";
import { Popover, PopoverClose, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import type { ThemeSetting } from "@/hooks/useThemeSettings";

const THEME_LABELS: Record<ThemeSetting, string> = {
  system:    "System",
  latte:     "Latte",
  frappe:    "Frappé",
  macchiato: "Macchiato",
  mocha:     "Mocha",
};

const ALL_SETTINGS: ThemeSetting[] = ["system", "latte", "frappe", "macchiato", "mocha"];

interface ThemePickerProps {
  theme: ThemeSetting;
  setTheme: (t: ThemeSetting) => void;
}

export const ThemePicker: React.FC<ThemePickerProps> = ({ theme, setTheme }) => {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={`Theme: ${THEME_LABELS[theme]}`}
          aria-haspopup="dialog"
          className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring rounded"
        >
          {THEME_LABELS[theme]}
          <svg className="w-3 h-3 opacity-60" viewBox="0 0 12 12" fill="none" aria-hidden>
            <path d="M2 4l4 4 4-4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
          </svg>
        </button>
      </PopoverTrigger>
      <PopoverContent side="top" align="end" className="w-36 p-1">
        {ALL_SETTINGS.map((t) => (
          <PopoverClose asChild key={t}>
            <button
              type="button"
              onClick={() => setTheme(t)}
              className={cn(
                "flex items-center gap-2 w-full text-xs px-2 py-1.5 rounded-sm transition-colors",
                "hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:bg-accent",
                t === theme ? "text-foreground font-medium" : "text-muted-foreground",
              )}
            >
              <CheckIcon
                className={cn("w-3 h-3 flex-shrink-0", t !== theme && "invisible")}
              />
              {THEME_LABELS[t]}
            </button>
          </PopoverClose>
        ))}
      </PopoverContent>
    </Popover>
  );
};
