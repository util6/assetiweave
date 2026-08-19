import { defaultSettings, normalizeStoredSettings, type AppSettings } from "./settingsSchema";

export const SETTINGS_CACHE_KEY = "assetiweave.settings";

export function readCachedSettings(): AppSettings {
  try {
    if (typeof localStorage === "undefined") {
      return defaultSettings;
    }
    const stored = localStorage.getItem(SETTINGS_CACHE_KEY);
    return stored ? normalizeStoredSettings(JSON.parse(stored)) : defaultSettings;
  } catch {
    return defaultSettings;
  }
}

export function writeCachedSettings(settings: AppSettings) {
  try {
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(SETTINGS_CACHE_KEY, JSON.stringify(settings));
    }
  } catch {
    // The desktop settings file is canonical; cache failures do not block it.
  }
}
