import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  appSettingsKey,
  settingsQueryOptions,
} from "../store/settings/settingsQueries";
import { useAppSettings } from "../store/settings/useAppSettings";
import { ensureAppLocale } from "./localeBootstrap";
import type { AppLocale } from "./types";

export function I18nEffects() {
  const { i18n } = useTranslation();
  const { settings } = useAppSettings();
  const queryClient = useQueryClient();
  const query = useQuery(settingsQueryOptions());
  const bootstrappedRef = useRef(false);

  // Synchronize document.documentElement.lang
  useEffect(() => {
    if (typeof document !== "undefined") {
      document.documentElement.lang = i18n.language === "zh" ? "zh-CN" : "en";
    }
  }, [i18n.language]);

  // Synchronize i18next language with settings draft or confirmed locale
  useEffect(() => {
    if (settings.locale && settings.locale !== i18n.language) {
      void i18n.changeLanguage(settings.locale);
    }
  }, [settings.locale, i18n]);

  // Handle one-time locale migration from localStorage to SQLite if unset
  useEffect(() => {
    if (bootstrappedRef.current || !query.data) {
      return;
    }
    bootstrappedRef.current = true;

    const currentFile = query.data;
    const currentLocale = (currentFile.settings as { locale?: AppLocale | null } | undefined)
      ?.locale;

    if (typeof window !== "undefined" && window.localStorage) {
      void ensureAppLocale(currentFile, window.localStorage).then((updatedFile) => {
        const nextLocale = (updatedFile.settings as { locale?: AppLocale | null } | undefined)
          ?.locale;
        if (nextLocale && nextLocale !== currentLocale) {
          queryClient.setQueryData(appSettingsKey, updatedFile);
          if (nextLocale !== i18n.language) {
            void i18n.changeLanguage(nextLocale);
          }
        }
      }).catch((err) => {
        console.error("Failed to bootstrap locale to SQLite settings:", err);
      });
    }
  }, [query.data, queryClient, i18n]);

  return null;
}
