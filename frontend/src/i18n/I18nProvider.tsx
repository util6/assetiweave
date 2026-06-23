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
    writeStoredLocale(locale);
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
  const stored = readStoredLocale();
  if (stored === "zh" || stored === "en") {
    return stored;
  }

  if (typeof navigator === "undefined") {
    return "zh";
  }

  return navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en";
}

function readStoredLocale(): string | null {
  if (typeof localStorage === "undefined") {
    return null;
  }

  try {
    return localStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

function writeStoredLocale(locale: Locale): void {
  if (typeof localStorage === "undefined") {
    return;
  }

  try {
    localStorage.setItem(STORAGE_KEY, locale);
  } catch {
    // Ignore restricted storage environments, such as browser privacy modes and Node tests.
  }
}

function interpolate(template: string, params?: TranslationParams) {
  if (!params) {
    return template;
  }

  return template.replace(/\{\{(\w+)\}\}/g, (_, key: string) => String(params[key] ?? ""));
}
