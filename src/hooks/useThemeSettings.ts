import { useCallback, useEffect, useMemo, useState } from "react";
import { hexToRgb, readableInk } from "@/lib/colorScale";

export type ThemeFlavor = "latte" | "frappe" | "macchiato" | "mocha";
export type ThemeSetting = "system" | ThemeFlavor;
export type AccentColor =
  | "rosewater" | "flamingo" | "pink" | "mauve" | "red" | "maroon"
  | "peach" | "yellow" | "green" | "teal" | "sky" | "sapphire"
  | "blue" | "lavender";

const ALL_FLAVORS: ThemeFlavor[] = ["latte", "frappe", "macchiato", "mocha"];
const ALL_SETTINGS: ThemeSetting[] = ["system", ...ALL_FLAVORS];
export const ALL_ACCENTS: AccentColor[] = [
  "rosewater", "flamingo", "pink", "mauve", "red", "maroon",
  "peach", "yellow", "green", "teal", "sky", "sapphire", "blue", "lavender",
];

const STORAGE_KEY = "diskviz-theme";
const ACCENT_STORAGE_KEY = "diskviz-accent";
const DEFAULT_SETTING: ThemeSetting = "system";
const DEFAULT_ACCENT: AccentColor = "blue";

export const ACCENT_COLORS: Record<ThemeFlavor, Record<AccentColor, string>> = {
  latte: {
    rosewater: "#dc8a78", flamingo: "#dd7878", pink:     "#ea76cb",
    mauve:     "#8839ef", red:      "#d20f39", maroon:   "#e64553",
    peach:     "#fe640b", yellow:   "#df8e1d", green:    "#40a02b",
    teal:      "#179299", sky:      "#04a5e5", sapphire: "#209fb5",
    blue:      "#1e66f5", lavender: "#7287fd",
  },
  frappe: {
    rosewater: "#f2d5cf", flamingo: "#eebebe", pink:     "#f4b8e4",
    mauve:     "#ca9ee6", red:      "#e78284", maroon:   "#ea999c",
    peach:     "#ef9f76", yellow:   "#e5c890", green:    "#a6d189",
    teal:      "#81c8be", sky:      "#99d1db", sapphire: "#85c1dc",
    blue:      "#8caaee", lavender: "#babbf1",
  },
  macchiato: {
    rosewater: "#f4dbd6", flamingo: "#f0c6c6", pink:     "#f5bde6",
    mauve:     "#c6a0f6", red:      "#ed8796", maroon:   "#ee99a0",
    peach:     "#f5a97f", yellow:   "#eed49f", green:    "#a6da95",
    teal:      "#8bd5ca", sky:      "#91d7e3", sapphire: "#7dc4e4",
    blue:      "#8aadf4", lavender: "#b7bdf8",
  },
  mocha: {
    rosewater: "#f5e0dc", flamingo: "#f2cdcd", pink:     "#f5c2e7",
    mauve:     "#cba6f7", red:      "#f38ba8", maroon:   "#eba0ac",
    peach:     "#fab387", yellow:   "#f9e2af", green:    "#a6e3a1",
    teal:      "#94e2d5", sky:      "#89dceb", sapphire: "#74c7ec",
    blue:      "#89b4fa", lavender: "#b4befe",
  },
};

/**
 * First 4 stops of the treemap size ramp per flavor (ramp-5 = current accent).
 * Progresses from near-background (small files) toward vivid (large files).
 */
export const VIZ_RAMP_BASE: Record<ThemeFlavor, readonly [string, string, string, string]> = {
  latte:     ["#ccd0da", "#acb0be", "#7287fd", "#209fb5"],
  frappe:    ["#414559", "#626880", "#babbf1", "#85c1dc"],
  macchiato: ["#363a4f", "#5b6078", "#b7bdf8", "#7dc4e4"],
  mocha:     ["#313244", "#585b70", "#b4befe", "#74c7ec"],
};

/**
 * Activeness ramp per flavor: fresh → old as green → yellow → peach → red.
 * Used by the "color by activeness" mode; independent of the accent/size ramp.
 */
export const VIZ_AGE_BASE: Record<ThemeFlavor, readonly string[]> = {
  latte:     ["#40a02b", "#df8e1d", "#fe640b", "#d20f39"],
  frappe:    ["#a6d189", "#e5c890", "#ef9f76", "#e78284"],
  macchiato: ["#a6da95", "#eed49f", "#f5a97f", "#ed8796"],
  mocha:     ["#a6e3a1", "#f9e2af", "#fab387", "#f38ba8"],
};

/**
 * Sunburst level colors per flavor (7 stops cycling by depth level).
 * These are static — they do NOT change with accent.
 */
export const VIZ_SUN_COLORS: Record<ThemeFlavor, readonly string[]> = {
  latte:     ["#1e66f5", "#209fb5", "#179299", "#40a02b", "#df8e1d", "#fe640b", "#8839ef", "#e64553"],
  frappe:    ["#8caaee", "#85c1dc", "#81c8be", "#a6d189", "#e5c890", "#ef9f76", "#ca9ee6", "#e78284"],
  macchiato: ["#8aadf4", "#7dc4e4", "#8bd5ca", "#a6da95", "#eed49f", "#f5a97f", "#c6a0f6", "#ed8796"],
  mocha:     ["#89b4fa", "#74c7ec", "#94e2d5", "#a6e3a1", "#f9e2af", "#fab387", "#cba6f7", "#f38ba8"],
};

function isValidSetting(v: string | null): v is ThemeSetting {
  return v !== null && (ALL_SETTINGS as string[]).includes(v);
}

function isValidAccent(v: string | null): v is AccentColor {
  return v !== null && (ALL_ACCENTS as string[]).includes(v);
}

function resolveSystemFlavor(): ThemeFlavor {
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "mocha" : "latte";
}

function applyFlavor(flavor: ThemeFlavor) {
  const root = document.documentElement;
  for (const f of ALL_FLAVORS) root.classList.remove(`theme-${f}`);
  root.classList.add(`theme-${flavor}`);
}

function applyAccentOverride(flavor: ThemeFlavor, accent: AccentColor) {
  const color = ACCENT_COLORS[flavor][accent];
  const fg = readableInk(hexToRgb(color));
  const root = document.documentElement;
  root.style.setProperty("--primary", color);
  root.style.setProperty("--primary-foreground", fg);
  root.style.setProperty("--ring", color);
  root.style.setProperty("--viz-ramp-5", color);
}

export function useThemeSettings() {
  const [setting, setSetting] = useState<ThemeSetting>(DEFAULT_SETTING);
  const [accent, setAccentState] = useState<AccentColor>(DEFAULT_ACCENT);

  useEffect(() => {
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (isValidSetting(stored)) setSetting(stored);
      const storedAccent = localStorage.getItem(ACCENT_STORAGE_KEY);
      if (isValidAccent(storedAccent)) setAccentState(storedAccent);
    } catch {
      // localStorage unavailable
    }
  }, []);

  useEffect(() => {
    const applyAll = (flavor: ThemeFlavor) => {
      applyFlavor(flavor);
      applyAccentOverride(flavor, accent);
    };

    if (setting !== "system") {
      applyAll(setting);
      return;
    }

    applyAll(resolveSystemFlavor());
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = () => applyAll(resolveSystemFlavor());
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, [setting, accent]);

  const setTheme = useCallback((next: ThemeSetting) => {
    setSetting(next);
    try { localStorage.setItem(STORAGE_KEY, next); } catch { /* ignore */ }
  }, []);

  const setAccent = useCallback((next: AccentColor) => {
    setAccentState(next);
    try { localStorage.setItem(ACCENT_STORAGE_KEY, next); } catch { /* ignore */ }
  }, []);

  const resolvedFlavor = useMemo<ThemeFlavor>(() => {
    if (setting !== "system") return setting;
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "mocha" : "latte";
  }, [setting]);

  const accentColor = useMemo(
    () => ACCENT_COLORS[resolvedFlavor][accent],
    [resolvedFlavor, accent],
  );

  const ageRampStops = useMemo(
    () => [...VIZ_AGE_BASE[resolvedFlavor]],
    [resolvedFlavor],
  );

  return { theme: setting, setTheme, accent, setAccent, resolvedFlavor, accentColor, ageRampStops };
}
