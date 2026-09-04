import { useCallback } from "react";
import { useTranslation } from "react-i18next";
import { useAppSettings } from "../store/settings/useAppSettings";
import type { AppLocale, TranslationKey, TranslationParams, Translator } from "./types";

export type { Translator };

export function useI18n(): {
  locale: AppLocale;
  setLocale(locale: AppLocale): void;
  t: Translator;
} {
  const { t: nativeT, i18n } = useTranslation();
  const { settings, updateSetting } = useAppSettings();

  const currentLocale: AppLocale =
    settings.locale ??
    ((i18n.language === "en" || i18n.language === "zh")
      ? i18n.language
      : "zh");

  const setLocale = useCallback(
    (locale: AppLocale) => {
      void i18n.changeLanguage(locale);
      updateSetting("locale", locale);
    },
    [i18n, updateSetting],
  );

  const t: Translator = useCallback(
    (key: TranslationKey, params?: TranslationParams) => {
      return nativeT(key, params as Record<string, unknown>);
    },
    [nativeT],
  );

  return {
    locale: currentLocale,
    setLocale,
    t,
  };
}
