import type { ReactNode } from "react";
import { I18nProvider } from "../i18n/I18nProvider";
import { AppSettingsProvider } from "../store/settings/AppSettingsProvider";
import { ConversationSyncProvider } from "./backgroundTasks/ConversationSyncProvider";
import { SkillBackupProvider } from "./backgroundTasks/SkillBackupProvider";
import { AppUpdateProvider } from "./updates/AppUpdateProvider";

export function AppProviders({ children }: { children: ReactNode }) {
  return (
    <I18nProvider>
      <AppSettingsProvider>
        <ConversationSyncProvider>
          <SkillBackupProvider>
            <AppUpdateProvider>{children}</AppUpdateProvider>
          </SkillBackupProvider>
        </ConversationSyncProvider>
      </AppSettingsProvider>
    </I18nProvider>
  );
}
