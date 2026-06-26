import { createContext, useContext, useEffect, useLayoutEffect, useMemo, useState, type ReactNode } from "react";

import { getAppSettings, saveAppSettings } from "../../services/appSettings";
import { applyThemeToElement } from "../../theme/cssVars";
import {
  defaultSettings,
  defaultStorageInfo,
  normalizeStoredSettings,
  resolveFontFamilyCss,
  type AppSettings,
  type AppSettingsStorageInfo,
} from "./settingsSchema";

export {
  COLUMN_MIN_WIDTH_MAX,
  COLUMN_MIN_WIDTH_MIN,
  COLUMN_MIN_WIDTH_STEP,
  DEFAULT_COLUMN_MIN_WIDTH,
  DEFAULT_CONVERSATION_CONTENT_CARD_COLORS,
  DEFAULT_CONVERSATION_TRANSLATION_TARGET_LANGUAGE,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  FONT_SIZE_STEP,
  DEFAULT_RESULT_PREVIEW_LINE_LIMIT,
  RESULT_PREVIEW_LINE_LIMIT_MAX,
  RESULT_PREVIEW_LINE_LIMIT_MIN,
  RESULT_PREVIEW_LINE_LIMIT_STEP,
  createFontFamilySetting,
  fontFamilyCss,
  fontFamilyOptionForPreset,
  fontFamilyOptions,
  firstFontFamilyName,
  normalizeStoredSettings,
  normalizeConversationTranslationTargetLanguage,
  resolveFontFamilyCss,
  TRANSLATION_TARGET_LANGUAGE_MAX_LENGTH,
} from "./settingsSchema";
export type {
  AppSettings,
  AppSettingsStorageInfo,
  ConversationContentCardColorSettings,
  ConversationTranslationTargetLanguage,
  ConversationRuntimeOverrideSettings,
  DataBackupSettings,
  FontFallbackKind,
  FontFamilyPresetId,
  FontFamilyValue,
  InterfaceDensity,
  SettingsPanelId,
} from "./settingsSchema";

const STORAGE_KEY = "assetiweave.settings";

interface AppSettingsContextValue {
  resetSettings: () => void;
  settings: AppSettings;
  settingsError: string | null;
  settingsLoaded: boolean;
  storageInfo: AppSettingsStorageInfo;
  updateSetting: <Key extends keyof AppSettings>(key: Key, value: AppSettings[Key]) => void;
}

const AppSettingsContext = createContext<AppSettingsContextValue | null>(null);

export function AppSettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<AppSettings>(() => readStoredSettings());
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [storageInfo, setStorageInfo] = useState<AppSettingsStorageInfo>(defaultStorageInfo);

  useEffect(() => {
    let cancelled = false;

    getAppSettings()
      .then((file) => {
        if (cancelled) return;
        setSettings(normalizeStoredSettings(file.settings));
        setStorageInfo({
          ...defaultStorageInfo,
          configDir: file.config_dir,
          configPath: file.config_path,
          conversationAdapterDir: file.conversation_adapter_dir,
        });
        setSettingsError(null);
        setSettingsLoaded(true);
      })
      .catch((error) => {
        if (cancelled) return;
        setSettingsError(errorMessage(error));
        setSettingsLoaded(true);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    writeStoredSettings(settings);
    if (!settingsLoaded || settingsError) return;
    void saveAppSettings(settings)
      .then((file) => {
        setStorageInfo({
          ...defaultStorageInfo,
          configDir: file.config_dir,
          configPath: file.config_path,
          conversationAdapterDir: file.conversation_adapter_dir,
        });
      })
      .catch((error) => setSettingsError(errorMessage(error)));
  }, [settings, settingsError, settingsLoaded]);

  useLayoutEffect(() => {
    document.documentElement.dataset.density = settings.density;
    document.documentElement.dataset.motion = settings.reduceMotion ? "reduced" : "full";
    document.documentElement.style.setProperty(
      "--app-font-family",
      resolveFontFamilyCss(settings.typography.interfaceFontFamily, "sans"),
    );
    document.documentElement.style.setProperty(
      "--app-content-font-family",
      resolveFontFamilyCss(settings.typography.contentFontFamily, "sans"),
    );
    document.documentElement.style.setProperty(
      "--app-code-font-family",
      resolveFontFamilyCss(settings.typography.codeFontFamily, "mono"),
    );
    document.documentElement.style.setProperty(
      "--app-base-font-size",
      `${settings.typography.baseFontSize}px`,
    );
    document.documentElement.style.setProperty(
      "--app-content-font-size",
      `${settings.typography.contentFontSize}px`,
    );
    document.documentElement.style.setProperty(
      "--app-code-font-size",
      `${settings.typography.codeFontSize}px`,
    );
    applyThemeToElement(document.documentElement, settings.theme);
  }, [settings.density, settings.reduceMotion, settings.theme, settings.typography]);

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
      settingsError,
      settingsLoaded,
      storageInfo,
      updateSetting,
    };
  }, [settings, settingsError, settingsLoaded, storageInfo]);

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
    if (typeof localStorage === "undefined") {
      return defaultSettings;
    }

    const stored = localStorage.getItem(STORAGE_KEY);
    if (!stored) {
      return defaultSettings;
    }

    return normalizeStoredSettings(JSON.parse(stored));
  } catch {
    return defaultSettings;
  }
}

function writeStoredSettings(settings: AppSettings) {
  try {
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
    }
  } catch {
    // The desktop JSON settings file remains the source of truth when available.
  }
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
