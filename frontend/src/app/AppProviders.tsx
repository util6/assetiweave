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
import { CatalogTaskProvider } from "./backgroundTasks/CatalogTaskProvider";
import { ConversationDataMaintenanceProvider } from "./backgroundTasks/ConversationDataMaintenanceProvider";
import { TeamTaskProvider } from "./backgroundTasks/TeamTaskProvider";

export function AppProviders({ children }: { children: ReactNode }) {
  return (
    <I18nProvider>
      <AppSettingsProvider>
        <ConversationCardKindRegistryProvider>
          <ConversationSyncProvider>
            <ConversationDataMaintenanceProvider>
              <AiExecutionTaskProvider>
                <AgentLifecycleTaskProvider>
                  <MemoryTaskProvider automaticDreamEnabled={false}>
                    <SearchIndexProvider>
                      <SkillBackupProvider>
                        <CatalogTaskProvider>
                          <TeamTaskProvider>
                            <AppUpdateProvider>{children}</AppUpdateProvider>
                          </TeamTaskProvider>
                        </CatalogTaskProvider>
                      </SkillBackupProvider>
                    </SearchIndexProvider>
                  </MemoryTaskProvider>
                </AgentLifecycleTaskProvider>
              </AiExecutionTaskProvider>
            </ConversationDataMaintenanceProvider>
          </ConversationSyncProvider>
        </ConversationCardKindRegistryProvider>
      </AppSettingsProvider>
    </I18nProvider>
  );
}
