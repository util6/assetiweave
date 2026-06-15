import type { ReactNode } from "react";
import { I18nProvider } from "../i18n/I18nProvider";
import { AppSettingsProvider } from "../store/settings/AppSettingsProvider";
import { ConversationSyncProvider } from "./backgroundTasks/ConversationSyncProvider";
import { AppUpdateProvider } from "./updates/AppUpdateProvider";

export function AppProviders({ children }: { children: ReactNode }) {
  return (
    <I18nProvider>
      <AppSettingsProvider>
        <ConversationSyncProvider>
          <AppUpdateProvider>{children}</AppUpdateProvider>
        </ConversationSyncProvider>
      </AppSettingsProvider>
    </I18nProvider>
  );
}
