import { useContext, useRef, type ReactNode } from "react";
import { I18nextProvider } from "react-i18next";
import type { i18n as I18nInstance } from "i18next";
import {
  QueryClient,
  QueryClientContext,
  QueryClientProvider,
} from "@tanstack/react-query";
import { createAppI18nSync } from "./createAppI18n";
import { resolveInitialLocale } from "./localeBootstrap";
import type { Translator } from "./types";

export { useI18n } from "./useI18n";
export type { Translator };

const fallbackQueryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: false },
  },
});

interface I18nProviderProps {
  children: ReactNode;
  i18n?: I18nInstance;
}

export function I18nProvider({ children, i18n }: I18nProviderProps) {
  const hasQueryClient = Boolean(useContext(QueryClientContext));
  const instanceRef = useRef<I18nInstance | null>(null);

  if (i18n) {
    instanceRef.current = i18n;
  } else if (!instanceRef.current) {
    let storedLocale: string | null = null;
    try {
      storedLocale =
        typeof localStorage !== "undefined"
          ? localStorage.getItem("assetiweave.locale")
          : null;
    } catch {
      // Ignore storage errors in restricted contexts
    }
    const initialLocale = resolveInitialLocale(storedLocale);
    instanceRef.current = createAppI18nSync(initialLocale);
  }

  const content = (
    <I18nextProvider i18n={instanceRef.current}>{children}</I18nextProvider>
  );

  if (!hasQueryClient) {
    return (
      <QueryClientProvider client={fallbackQueryClient}>
        {content}
      </QueryClientProvider>
    );
  }

  return content;
}
