import type { ReactNode } from "react";
import { I18nProvider } from "../i18n/I18nProvider";
import { AppSettingsProvider } from "../store/settings/AppSettingsProvider";
import { ConversationSyncProvider } from "./backgroundTasks/ConversationSyncProvider";
import { AiExecutionTaskProvider } from "./backgroundTasks/AiExecutionTaskProvider";
import { AgentLifecycleTaskProvider } from "./backgroundTasks/AgentLifecycleTaskProvider";
import { SearchIndexProvider } from "./backgroundTasks/SearchIndexProvider";
import { SkillBackupProvider } from "./backgroundTasks/SkillBackupProvider";
import { MemoryTaskProvider } from "./backgroundTasks/MemoryTaskProvider";
import { AppUpdateProvider } from "./updates/AppUpdateProvider";
import { ConversationCardKindRegistryProvider } from "../components/conversations/ConversationCardKindRegistry";

export function AppProviders({ children }: { children: ReactNode }) {
  return (
    <I18nProvider>
      <AppSettingsProvider>
        <ConversationCardKindRegistryProvider>
          <ConversationSyncProvider>
            <AiExecutionTaskProvider>
              <AgentLifecycleTaskProvider>
                <MemoryTaskProvider>
                  <SearchIndexProvider>
                    <SkillBackupProvider>
                      <AppUpdateProvider>{children}</AppUpdateProvider>
                    </SkillBackupProvider>
                  </SearchIndexProvider>
                </MemoryTaskProvider>
              </AgentLifecycleTaskProvider>
            </AiExecutionTaskProvider>
          </ConversationSyncProvider>
        </ConversationCardKindRegistryProvider>
      </AppSettingsProvider>
    </I18nProvider>
  );
}
