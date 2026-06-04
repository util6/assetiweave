import type { ReactNode } from "react";
import { I18nProvider } from "../i18n/I18nProvider";
import { AppSettingsProvider } from "../store/settings/AppSettingsProvider";
import { AppUpdateProvider } from "./updates/AppUpdateProvider";

export function AppProviders({ children }: { children: ReactNode }) {
  return (
    <I18nProvider>
      <AppSettingsProvider>
        <AppUpdateProvider>{children}</AppUpdateProvider>
      </AppSettingsProvider>
    </I18nProvider>
  );
}
