import type { AppLocale } from "../store/settings/settingsSchema";
import type { TranslationKey } from "./messages";

export type { AppLocale };
export type Locale = AppLocale;

export type { TranslationKey };
export type TranslationParams = Record<string, string | number>;

export type Translator = (
  key: TranslationKey,
  params?: TranslationParams,
) => string;
