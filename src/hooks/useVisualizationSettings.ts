import { useCallback, useEffect, useState } from "react";

export type VisualizationType = "treemap" | "sunburst";

interface VisualizationSettings {
  type: VisualizationType;
}

const DEFAULT_SETTINGS: VisualizationSettings = {
  type: "treemap",
};

const STORAGE_KEY = "diskviz-visualization-settings";

export function useVisualizationSettings() {
  const [settings, setSettings] = useState<VisualizationSettings>(DEFAULT_SETTINGS);

  useEffect(() => {
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (stored) {
        const parsed = JSON.parse(stored) as VisualizationSettings;
        if (parsed.type === "treemap" || parsed.type === "sunburst") {
          setSettings(parsed);
        }
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

  return {
    settings,
    setVisualizationType,
    visualizationType: settings.type,
  };
}
