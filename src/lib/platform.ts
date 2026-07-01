import { platform } from "@tauri-apps/plugin-os";

/**
 * Platform-aware modifier-key labels and shortcut formatting.
 * `platform()` is synchronous in Tauri v2 (reads a value injected at
 * startup), so these can be computed once at module load.
 */
const currentPlatform = platform();

export const isMac = currentPlatform === "macos";
export const isWindows = currentPlatform === "windows";

/** "⌘" on macOS, "Ctrl" everywhere else. */
export const modLabel = isMac ? "⌘" : "Ctrl";

/** "⇧" on macOS, "Shift" everywhere else. */
export const shiftLabel = isMac ? "⇧" : "Shift";

/**
 * Format a shortcut combo for display, e.g. `combo("O")` → "⌘O" / "Ctrl+O",
 * `combo("Z", { shift: true })` → "⌘⇧Z" / "Ctrl+Shift+Z".
 */
export function combo(key: string, opts?: { shift?: boolean }): string {
  const parts = [modLabel];
  if (opts?.shift) parts.push(shiftLabel);
  parts.push(key);
  return isMac ? parts.join("") : parts.join("+");
}
