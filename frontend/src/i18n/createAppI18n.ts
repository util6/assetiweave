import { createInstance, type i18n } from "i18next";
import { initReactI18next } from "react-i18next";
import { messages } from "./messages";
import type { AppLocale } from "./types";

export function createAppI18nSync(locale: AppLocale): i18n {
  const instance = createInstance();
  void instance.use(initReactI18next).init({
    lng: locale,
    fallbackLng: "zh",
    resources: {
      zh: { translation: messages.zh },
      en: { translation: messages.en },
    },
    keySeparator: false,
    nsSeparator: false,
    returnNull: false,
    interpolation: { escapeValue: false },
  });
  return instance;
}

export async function createAppI18n(locale: AppLocale): Promise<i18n> {
  return createAppI18nSync(locale);
}
