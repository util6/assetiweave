import type { ReactNode } from "react";
import { I18nProvider } from "../i18n/I18nProvider";
import { AppSettingsProvider } from "../store/settings/AppSettingsProvider";
import { ConversationSyncProvider } from "./backgroundTasks/ConversationSyncProvider";
import { SearchIndexProvider } from "./backgroundTasks/SearchIndexProvider";
import { SkillBackupProvider } from "./backgroundTasks/SkillBackupProvider";
import { AppUpdateProvider } from "./updates/AppUpdateProvider";

export function AppProviders({ children }: { children: ReactNode }) {
  return (
    <I18nProvider>
      <AppSettingsProvider>
        <ConversationSyncProvider>
          <SearchIndexProvider>
            <SkillBackupProvider>
              <AppUpdateProvider>{children}</AppUpdateProvider>
            </SkillBackupProvider>
          </SearchIndexProvider>
        </ConversationSyncProvider>
      </AppSettingsProvider>
    </I18nProvider>
  );
}
