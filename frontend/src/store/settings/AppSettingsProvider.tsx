import { createContext, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";

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
import { readCachedSettings, writeCachedSettings } from "./settingsPersistence";

export {
  AUTO_DREAM_MIN_HOURS_MAX,
  AUTO_DREAM_MIN_HOURS_MIN,
  AUTO_DREAM_MIN_SESSIONS_MAX,
  AUTO_DREAM_MIN_SESSIONS_MIN,
  COLUMN_MIN_WIDTH_MAX,
  COLUMN_MIN_WIDTH_MIN,
  COLUMN_MIN_WIDTH_STEP,
  DEFAULT_COLUMN_MIN_WIDTH,
  DEFAULT_AUTO_DREAM_MIN_HOURS,
  DEFAULT_AUTO_DREAM_MIN_SESSIONS,
  DEFAULT_CONVERSATION_CONTENT_CARD_COLORS,
  DEFAULT_CONVERSATION_TRANSLATION_TARGET_LANGUAGE,
  DEFAULT_CONVERSATION_TRANSLATION_PROMPT_TEMPLATE,
  DEFAULT_PROMPT_OPTIMIZATION_PROMPT_TEMPLATE,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  FONT_SIZE_STEP,
  DEFAULT_RESULT_PREVIEW_LINE_LIMIT,
  RESULT_PREVIEW_LINE_LIMIT_MAX,
  RESULT_PREVIEW_LINE_LIMIT_MIN,
  RESULT_PREVIEW_LINE_LIMIT_STEP,
  assignAgentToAction,
  assignModelToAgentActions,
  createFontFamilySetting,
  fontFamilyCss,
  fontFamilyOptionForPreset,
  fontFamilyOptions,
  firstFontFamilyName,
  normalizeStoredSettings,
  normalizeConversationTranslationTargetLanguage,
  modelsByAgentFromAssignments,
  resolveAgentCapability,
  resolveFontFamilyCss,
  TRANSLATION_TARGET_LANGUAGE_MAX_LENGTH,
  TRANSLATION_MODEL_MAX_LENGTH,
  TRANSLATION_PROMPT_TEMPLATE_MAX_LENGTH,
  PROMPT_OPTIMIZATION_PROMPT_TEMPLATE_MAX_LENGTH,
} from "./settingsSchema";
export type {
  AgentActionId,
  AgentAssignment,
  AgentAssignments,
  AgentCapabilityAssignments,
  AgentCapabilityServiceId,
  AiRuntimeCli,
  AiRuntimeSettings,
  AppSettings,
  AppSettingsStorageInfo,
  ConversationContentCardColorSettings,
  ConversationTranslationCli,
  ConversationTranslationProvider,
  ConversationTranslationSettings,
  ConversationTranslationTargetLanguage,
  ConversationRuntimeOverrideSettings,
  DataBackupSettings,
  FontFallbackKind,
  FontFamilyPresetId,
  FontFamilyValue,
  InterfaceDensity,
  MemorySettings,
  PromptOptimizationSettings,
  ResolvedConversationTranslationSettings,
  SettingsPanelId,
} from "./settingsSchema";

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
  const [settings, setSettings] = useState<AppSettings>(() => readCachedSettings());
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [storageInfo, setStorageInfo] = useState<AppSettingsStorageInfo>(defaultStorageInfo);
  const lastPersistedSettingsRef = useRef<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    getAppSettings()
      .then((file) => {
        if (cancelled) return;
        const normalizedSettings = normalizeStoredSettings(file.settings);
        setSettings(normalizedSettings);
        writeCachedSettings(normalizedSettings);
        lastPersistedSettingsRef.current = JSON.stringify(normalizedSettings);
        setStorageInfo({
          ...defaultStorageInfo,
          configDir: file.display_config_dir ?? file.config_dir,
          configPath: file.display_config_path ?? file.config_path,
          conversationAdapterDir: file.display_conversation_adapter_dir ?? file.conversation_adapter_dir,
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
    if (!settingsLoaded || settingsError) return;
    const serializedSettings = JSON.stringify(settings);
    if (lastPersistedSettingsRef.current === serializedSettings) return;

    let active = true;
    lastPersistedSettingsRef.current = serializedSettings;
    void saveAppSettings(settings)
      .then((file) => {
        if (!active) return;
        const normalizedSettings = normalizeStoredSettings(file.settings);
        writeCachedSettings(normalizedSettings);
        lastPersistedSettingsRef.current = JSON.stringify(normalizedSettings);
        setSettings((current) => {
          if (!settingsEqual(current, settings)) return current;
          return settingsEqual(current, normalizedSettings) ? current : normalizedSettings;
        });
        setStorageInfo({
          ...defaultStorageInfo,
          configDir: file.display_config_dir ?? file.config_dir,
          configPath: file.display_config_path ?? file.config_path,
          conversationAdapterDir: file.display_conversation_adapter_dir ?? file.conversation_adapter_dir,
        });
        setSettingsError(null);
      })
      .catch((error) => {
        if (active) {
          lastPersistedSettingsRef.current = null;
          setSettingsError(errorMessage(error));
        }
      });
    return () => {
      active = false;
    };
  }, [settings, settingsError, settingsLoaded]);

  useLayoutEffect(() => {
    document.documentElement.dataset.density = settings.density;
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
  }, [settings.density, settings.theme, settings.typography]);

  const value = useMemo<AppSettingsContextValue>(() => {
    function updateSetting<Key extends keyof AppSettings>(key: Key, settingValue: AppSettings[Key]) {
      setSettingsError(null);
      lastPersistedSettingsRef.current = null;
      setSettings((currentSettings) => ({
        ...currentSettings,
        [key]: settingValue,
      }));
    }

    return {
      resetSettings: () => {
        setSettingsError(null);
        lastPersistedSettingsRef.current = null;
        setSettings(defaultSettings);
      },
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

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function settingsEqual(left: AppSettings, right: AppSettings) {
  return JSON.stringify(left) === JSON.stringify(right);
}
