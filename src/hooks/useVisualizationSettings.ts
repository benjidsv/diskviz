import { useCallback, useEffect, useState } from "react";

export type VisualizationType = "treemap" | "sunburst";
export type ColorMode = "size" | "activeness";

interface VisualizationSettings {
  type: VisualizationType;
  /** What drives tile/arc color: file size, or file age (activeness). */
  colorMode: ColorMode;
  /** Age (days) at which a folder is considered fully "old" in activeness mode. */
  ageThresholdDays: number;
}

const DEFAULT_SETTINGS: VisualizationSettings = {
  type: "treemap",
  colorMode: "size",
  ageThresholdDays: 730,
};

const STORAGE_KEY = "diskviz-visualization-settings";

export function useVisualizationSettings() {
  const [settings, setSettings] = useState<VisualizationSettings>(DEFAULT_SETTINGS);

  useEffect(() => {
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (stored) {
        const parsed = JSON.parse(stored) as Partial<VisualizationSettings>;
        setSettings((prev) => ({
          type: parsed.type === "treemap" || parsed.type === "sunburst" ? parsed.type : prev.type,
          colorMode:
            parsed.colorMode === "size" || parsed.colorMode === "activeness"
              ? parsed.colorMode
              : prev.colorMode,
          ageThresholdDays:
            typeof parsed.ageThresholdDays === "number" && parsed.ageThresholdDays > 0
              ? parsed.ageThresholdDays
              : prev.ageThresholdDays,
        }));
      }
    } catch (error) {
      console.warn("Failed to load visualization settings:", error);
    }
  }, []);

  const updateSettings = useCallback((next: Partial<VisualizationSettings>) => {
    setSettings((prev) => {
      const updated = { ...prev, ...next };
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(updated));
      } catch (error) {
        console.warn("Failed to save visualization settings:", error);
      }
      return updated;
    });
  }, []);

  const setVisualizationType = useCallback(
    (type: VisualizationType) => updateSettings({ type }),
    [updateSettings],
  );

  const setColorMode = useCallback(
    (colorMode: ColorMode) => updateSettings({ colorMode }),
    [updateSettings],
  );

  const setAgeThresholdDays = useCallback(
    (ageThresholdDays: number) => updateSettings({ ageThresholdDays }),
    [updateSettings],
  );

  return {
    settings,
    setVisualizationType,
    setColorMode,
    setAgeThresholdDays,
    visualizationType: settings.type,
    colorMode: settings.colorMode,
    ageThresholdDays: settings.ageThresholdDays,
  };
}
