import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

const STORAGE_KEY = "assetiweave.settings";

export type InterfaceDensity = "comfortable" | "compact";

export interface AppSettings {
  confirmBeforeDeploy: boolean;
  density: InterfaceDensity;
  reduceMotion: boolean;
  showStartupNotification: boolean;
}

interface AppSettingsContextValue {
  resetSettings: () => void;
  settings: AppSettings;
  updateSetting: <Key extends keyof AppSettings>(key: Key, value: AppSettings[Key]) => void;
}

const defaultSettings: AppSettings = {
  confirmBeforeDeploy: true,
  density: "comfortable",
  reduceMotion: false,
  showStartupNotification: true,
};

const AppSettingsContext = createContext<AppSettingsContextValue | null>(null);

export function AppSettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<AppSettings>(() => readStoredSettings());

  useEffect(() => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  }, [settings]);

  useEffect(() => {
    document.documentElement.dataset.density = settings.density;
    document.documentElement.dataset.motion = settings.reduceMotion ? "reduced" : "full";
  }, [settings.density, settings.reduceMotion]);

  const value = useMemo<AppSettingsContextValue>(() => {
    function updateSetting<Key extends keyof AppSettings>(key: Key, settingValue: AppSettings[Key]) {
      setSettings((currentSettings) => ({
        ...currentSettings,
        [key]: settingValue,
      }));
    }

    return {
      resetSettings: () => setSettings(defaultSettings),
      settings,
      updateSetting,
    };
  }, [settings]);

  return <AppSettingsContext.Provider value={value}>{children}</AppSettingsContext.Provider>;
}

export function useAppSettings() {
  const context = useContext(AppSettingsContext);
  if (!context) {
    throw new Error("useAppSettings must be used inside AppSettingsProvider");
  }
  return context;
}

function readStoredSettings(): AppSettings {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (!stored) {
      return defaultSettings;
    }

    return normalizeStoredSettings(JSON.parse(stored));
  } catch {
    return defaultSettings;
  }
}

function normalizeStoredSettings(value: unknown): AppSettings {
  if (!value || typeof value !== "object") {
    return defaultSettings;
  }

  const stored = value as Partial<AppSettings>;

  return {
    confirmBeforeDeploy: typeof stored.confirmBeforeDeploy === "boolean" ? stored.confirmBeforeDeploy : defaultSettings.confirmBeforeDeploy,
    density: stored.density === "compact" ? "compact" : defaultSettings.density,
    reduceMotion: typeof stored.reduceMotion === "boolean" ? stored.reduceMotion : defaultSettings.reduceMotion,
    showStartupNotification:
      typeof stored.showStartupNotification === "boolean" ? stored.showStartupNotification : defaultSettings.showStartupNotification,
  };
}
