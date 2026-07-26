import type { ReactNode } from "react";
import { I18nProvider } from "../i18n/I18nProvider";
import { AppSettingsProvider } from "../store/settings/AppSettingsProvider";
import { ConversationSyncProvider } from "./backgroundTasks/ConversationSyncProvider";
import { SearchIndexProvider } from "./backgroundTasks/SearchIndexProvider";
import { SkillBackupProvider } from "./backgroundTasks/SkillBackupProvider";
import { MemoryTaskProvider } from "./backgroundTasks/MemoryTaskProvider";
import { AppUpdateProvider } from "./updates/AppUpdateProvider";

export function AppProviders({ children }: { children: ReactNode }) {
  return (
    <I18nProvider>
      <AppSettingsProvider>
        <ConversationSyncProvider>
          <MemoryTaskProvider>
            <SearchIndexProvider>
              <SkillBackupProvider>
                <AppUpdateProvider>{children}</AppUpdateProvider>
              </SkillBackupProvider>
            </SearchIndexProvider>
          </MemoryTaskProvider>
        </ConversationSyncProvider>
      </AppSettingsProvider>
    </I18nProvider>
  );
}
