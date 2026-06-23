import { useCallback, useEffect, useState } from "react";

export type ThemeFlavor = "latte" | "frappe" | "macchiato" | "mocha";
export type ThemeSetting = "system" | ThemeFlavor;

const ALL_FLAVORS: ThemeFlavor[] = ["latte", "frappe", "macchiato", "mocha"];
const ALL_SETTINGS: ThemeSetting[] = ["system", ...ALL_FLAVORS];
const STORAGE_KEY = "diskviz-theme";
const DEFAULT_SETTING: ThemeSetting = "system";

function isValidSetting(v: string | null): v is ThemeSetting {
  return v !== null && (ALL_SETTINGS as string[]).includes(v);
}

function resolveSystemFlavor(): ThemeFlavor {
  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "mocha"
    : "latte";
}

function applyFlavor(flavor: ThemeFlavor) {
  const root = document.documentElement;
  for (const f of ALL_FLAVORS) {
    root.classList.remove(`theme-${f}`);
  }
  root.classList.add(`theme-${flavor}`);
}

export function useThemeSettings() {
  const [setting, setSetting] = useState<ThemeSetting>(DEFAULT_SETTING);

  // Load persisted setting on mount.
  useEffect(() => {
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (isValidSetting(stored)) {
        setSetting(stored);
      }
    } catch {
      // localStorage unavailable — stay with default
    }
  }, []);

  // Apply the correct theme class and (for system) subscribe to OS changes.
  useEffect(() => {
    if (setting !== "system") {
      applyFlavor(setting);
      return;
    }
    // system mode: apply immediately, then track OS changes
    applyFlavor(resolveSystemFlavor());
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = () => applyFlavor(resolveSystemFlavor());
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, [setting]);

  const setTheme = useCallback((next: ThemeSetting) => {
    setSetting(next);
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // ignore
    }
  }, []);

  return { theme: setting, setTheme };
}
