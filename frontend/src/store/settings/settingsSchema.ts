import type { ThemeId } from "../../theme/schema";
import { normalizeThemeId } from "../../theme/themes";

export type InterfaceDensity = "comfortable" | "compact";

export type FontFamilyPresetId = "system" | "geist" | "serif" | "mono";
export type FontFamilyToken = FontFamilyPresetId;
export type FontFamilyValue = string;

export type SettingsPanelId =
  | "general.appearance"
  | "general.typography"
  | "general.storage"
  | "workspace.menu"
  | "workspace.shortcuts"
  | "workspace.deployment"
  | "workspace.notifications"
  | "conversations.sessions"
  | "conversations.adapters";

export interface FontFamilyOption {
  labelKey: string;
  value: FontFamilyValue;
}

export const fontFamilyCss: Record<FontFamilyPresetId, string> = {
  system:
    'ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
  geist: '"Geist", ui-sans-serif, system-ui, sans-serif',
  serif: 'Georgia, "Times New Roman", Times, serif',
  mono: '"JetBrains Mono", "SFMono-Regular", Consolas, monospace',
};

export const fontFamilyOptions: FontFamilyOption[] = [
  { labelKey: "settings.font.system", value: fontFamilyCss.system },
  { labelKey: "settings.font.geist", value: fontFamilyCss.geist },
  { labelKey: "settings.font.serif", value: fontFamilyCss.serif },
  { labelKey: "settings.font.mono", value: fontFamilyCss.mono },
];

export const COLUMN_MIN_WIDTH_MIN = 220;
export const COLUMN_MIN_WIDTH_MAX = 480;
export const COLUMN_MIN_WIDTH_STEP = 20;
export const DEFAULT_COLUMN_MIN_WIDTH = 280;

export const FONT_SIZE_MIN = 11;
export const FONT_SIZE_MAX = 20;
export const FONT_SIZE_STEP = 1;

export interface TypographySettings {
  baseFontSize: number;
  codeFontFamily: FontFamilyValue;
  codeFontSize: number;
  contentFontFamily: FontFamilyValue;
  contentFontSize: number;
  interfaceFontFamily: FontFamilyValue;
}

export interface ConversationPageSettings {
  contentFontFamily: FontFamilyValue;
  contentFontSize: number;
  codeFontSize: number;
  sessionBrowserFontFamily: FontFamilyValue;
  sessionBrowserFontSize: number;
  sessionToolbarCompact: boolean;
}

export interface AppSettings {
  columnMinWidth: number;
  confirmBeforeDeploy: boolean;
  density: InterfaceDensity;
  reduceMotion: boolean;
  showStartupNotification: boolean;
  theme: ThemeId;
  typography: TypographySettings;
  conversations: ConversationPageSettings;
}

export interface AppSettingsStorageInfo {
  configDir: string;
  configPath: string;
  conversationAdapterDir: string;
}

export const defaultSettings: AppSettings = {
  columnMinWidth: DEFAULT_COLUMN_MIN_WIDTH,
  confirmBeforeDeploy: true,
  density: "comfortable",
  reduceMotion: false,
  showStartupNotification: true,
  theme: "midnight",
  typography: {
    baseFontSize: 14,
    codeFontFamily: fontFamilyCss.mono,
    codeFontSize: 13,
    contentFontFamily: fontFamilyCss.system,
    contentFontSize: 14,
    interfaceFontFamily: fontFamilyCss.geist,
  },
  conversations: {
    codeFontSize: 13,
    contentFontFamily: fontFamilyCss.system,
    contentFontSize: 14,
    sessionBrowserFontFamily: fontFamilyCss.system,
    sessionBrowserFontSize: 13,
    sessionToolbarCompact: true,
  },
};

export const defaultStorageInfo: AppSettingsStorageInfo = {
  configDir: "~/.assetiweave",
  configPath: "~/.assetiweave/config.json",
  conversationAdapterDir: "~/.assetiweave/conversation-adapters",
};

export function normalizeStoredSettings(value: unknown): AppSettings {
  if (!value || typeof value !== "object") {
    return defaultSettings;
  }

  const stored = value as Partial<AppSettings>;
  const typography = normalizeTypographySettings(stored.typography);
  const conversations = normalizeConversationPageSettings(stored.conversations, typography);

  return {
    columnMinWidth: normalizeColumnMinWidth(stored.columnMinWidth),
    confirmBeforeDeploy:
      typeof stored.confirmBeforeDeploy === "boolean"
        ? stored.confirmBeforeDeploy
        : defaultSettings.confirmBeforeDeploy,
    density: stored.density === "compact" ? "compact" : defaultSettings.density,
    reduceMotion:
      typeof stored.reduceMotion === "boolean"
        ? stored.reduceMotion
        : defaultSettings.reduceMotion,
    showStartupNotification:
      typeof stored.showStartupNotification === "boolean"
        ? stored.showStartupNotification
        : defaultSettings.showStartupNotification,
    theme: normalizeThemeId(stored.theme),
    typography,
    conversations,
  };
}

function normalizeTypographySettings(value: unknown): TypographySettings {
  const stored = isRecord(value) ? (value as Partial<TypographySettings>) : {};
  return {
    baseFontSize: normalizeFontSize(
      stored.baseFontSize,
      defaultSettings.typography.baseFontSize,
    ),
    codeFontFamily: normalizeFontFamily(
      stored.codeFontFamily,
      defaultSettings.typography.codeFontFamily,
    ),
    codeFontSize: normalizeFontSize(
      stored.codeFontSize,
      defaultSettings.typography.codeFontSize,
    ),
    contentFontFamily: normalizeFontFamily(
      stored.contentFontFamily,
      defaultSettings.typography.contentFontFamily,
    ),
    contentFontSize: normalizeFontSize(
      stored.contentFontSize,
      defaultSettings.typography.contentFontSize,
    ),
    interfaceFontFamily: normalizeFontFamily(
      stored.interfaceFontFamily,
      defaultSettings.typography.interfaceFontFamily,
    ),
  };
}

function normalizeConversationPageSettings(
  value: unknown,
  typography: TypographySettings,
): ConversationPageSettings {
  const stored = isRecord(value) ? (value as Partial<ConversationPageSettings>) : {};
  return {
    codeFontSize: normalizeFontSize(stored.codeFontSize, typography.codeFontSize),
    contentFontFamily: normalizeFontFamily(
      stored.contentFontFamily,
      typography.contentFontFamily,
    ),
    contentFontSize: normalizeFontSize(
      stored.contentFontSize,
      typography.contentFontSize,
    ),
    sessionBrowserFontFamily: normalizeFontFamily(
      stored.sessionBrowserFontFamily,
      typography.contentFontFamily,
    ),
    sessionBrowserFontSize: normalizeFontSize(stored.sessionBrowserFontSize, 13),
    sessionToolbarCompact:
      typeof stored.sessionToolbarCompact === "boolean"
        ? stored.sessionToolbarCompact
        : defaultSettings.conversations.sessionToolbarCompact,
  };
}

function normalizeColumnMinWidth(value: unknown) {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return DEFAULT_COLUMN_MIN_WIDTH;
  }

  return clamp(value, COLUMN_MIN_WIDTH_MIN, COLUMN_MIN_WIDTH_MAX);
}

function normalizeFontSize(value: unknown, fallback: number) {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return fallback;
  }

  return clamp(Math.round(value), FONT_SIZE_MIN, FONT_SIZE_MAX);
}

export function resolveFontFamilyCss(value: FontFamilyValue) {
  return normalizeFontFamily(value, fontFamilyCss.system);
}

function normalizeFontFamily(value: unknown, fallback: FontFamilyValue): FontFamilyValue {
  if (typeof value !== "string") {
    return fallback;
  }

  const trimmedValue = value.trim().replace(/\s+/g, " ");
  const legacyPreset = fontFamilyCss[trimmedValue as FontFamilyPresetId];
  if (legacyPreset) {
    return legacyPreset;
  }

  if (!isValidFontFamilyValue(trimmedValue)) {
    return fallback;
  }

  return trimmedValue;
}

function isValidFontFamilyValue(value: string) {
  return value.length > 0 && value.length <= 180 && !/[;{}<>]/.test(value);
}

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max);
}

function isRecord(value: unknown) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}
