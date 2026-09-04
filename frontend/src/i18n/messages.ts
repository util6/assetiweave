import { catalogZh } from "./resources/zh/catalog";
import { commonZh } from "./resources/zh/common";
import { conversationsZh } from "./resources/zh/conversations";
import { memoryZh } from "./resources/zh/memory";
import { settingsZh } from "./resources/zh/settings";
import { teamZh } from "./resources/zh/team";

import { catalogEn } from "./resources/en/catalog";
import { commonEn } from "./resources/en/common";
import { conversationsEn } from "./resources/en/conversations";
import { memoryEn } from "./resources/en/memory";
import { settingsEn } from "./resources/en/settings";
import { teamEn } from "./resources/en/team";

import type { AppLocale, TranslationParams, Translator } from "./types";

const zh = {
  ...commonZh,
  ...catalogZh,
  ...conversationsZh,
  ...memoryZh,
  ...teamZh,
  ...settingsZh,
} as const;

export type TranslationKey = keyof typeof zh;
export type Locale = AppLocale;
export type { TranslationParams, Translator };

const en: Record<TranslationKey, string> = {
  ...commonEn,
  ...catalogEn,
  ...conversationsEn,
  ...memoryEn,
  ...teamEn,
  ...settingsEn,
};

export const messages: Record<Locale, Record<TranslationKey, string>> = {
  zh,
  en,
};
