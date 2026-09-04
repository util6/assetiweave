import {
  initializeAppLocaleIfUnset,
  type AppSettingsFile,
} from "../services/appSettings";
import type { AppLocale } from "./types";

const STORAGE_KEY = "assetiweave.locale";

export function resolveInitialLocale(
  stored: string | null,
  navigatorLanguage?: string,
): AppLocale {
  if (stored === "zh" || stored === "en") {
    return stored;
  }

  const lang =
    navigatorLanguage ??
    (typeof navigator !== "undefined" ? navigator.language : undefined);

  if (lang) {
    return lang.toLowerCase().startsWith("zh") ? "zh" : "en";
  }

  return "zh";
}

export async function ensureAppLocale(
  file: AppSettingsFile,
  storage: Pick<Storage, "getItem" | "removeItem">,
  navigatorLanguage?: string,
): Promise<AppSettingsFile> {
  const currentLocale = (
    file.settings as { locale?: AppLocale | null } | undefined
  )?.locale;

  if (currentLocale === "zh" || currentLocale === "en") {
    try {
      storage.removeItem(STORAGE_KEY);
    } catch {
      // Ignore storage errors
    }
    return file;
  }

  let stored: string | null = null;
  try {
    stored = storage.getItem(STORAGE_KEY);
  } catch {
    // Ignore storage errors and fallback to navigator
  }

  const candidate = resolveInitialLocale(stored, navigatorLanguage);
  const updatedFile = await initializeAppLocaleIfUnset(candidate);

  try {
    storage.removeItem(STORAGE_KEY);
  } catch {
    // Ignore storage errors
  }

  return updatedFile;
}
