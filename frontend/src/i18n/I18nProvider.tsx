import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { messages, type Locale, type TranslationKey, type TranslationParams } from "./messages";

const STORAGE_KEY = "assetiweave.locale";

export type Translator = (key: TranslationKey, params?: TranslationParams) => string;

interface I18nContextValue {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: Translator;
}

const I18nContext = createContext<I18nContextValue | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(() => getInitialLocale());

  useEffect(() => {
    document.documentElement.lang = locale === "zh" ? "zh-CN" : "en";
    localStorage.setItem(STORAGE_KEY, locale);
  }, [locale]);

  const value = useMemo<I18nContextValue>(() => {
    const t: Translator = (key, params) => interpolate(messages[locale][key] ?? messages.zh[key] ?? key, params);

    return {
      locale,
      setLocale: setLocaleState,
      t,
    };
  }, [locale]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n() {
  const context = useContext(I18nContext);
  if (!context) {
    throw new Error("useI18n must be used inside I18nProvider");
  }
  return context;
}

function getInitialLocale(): Locale {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === "zh" || stored === "en") {
    return stored;
  }

  return navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en";
}

function interpolate(template: string, params?: TranslationParams) {
  if (!params) {
    return template;
  }

  return template.replace(/\{\{(\w+)\}\}/g, (_, key: string) => String(params[key] ?? ""));
}
