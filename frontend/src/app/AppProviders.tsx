import type { ReactNode } from "react";
import { QueryClientProvider } from "@tanstack/react-query";
import { createAppQueryClient } from "./query/queryClient";
import { QueryScopeProvider } from "./query/QueryScopeProvider";
import { I18nProvider } from "../i18n/I18nProvider";
import { SettingsEffects } from "../store/settings/SettingsEffects";
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

const appQueryClient = createAppQueryClient();

export function AppProviders({ children }: { children: ReactNode }) {
  return (
    <QueryClientProvider client={appQueryClient}>
      <QueryScopeProvider>
        <I18nProvider>
          <SettingsEffects />
          <ConversationCardKindRegistryProvider>
            <ConversationSyncProvider>
              <ConversationDataMaintenanceProvider>
                <AiExecutionTaskProvider>
                  <AgentLifecycleTaskProvider>
                    <MemoryTaskProvider>
                      <SkillBackupProvider>
                        <CatalogTaskProvider>
                          <TeamTaskProvider>
                            <AppUpdateProvider>
                              <SearchIndexProvider />
                              {children}
                            </AppUpdateProvider>
                          </TeamTaskProvider>
                        </CatalogTaskProvider>
                      </SkillBackupProvider>
                    </MemoryTaskProvider>
                  </AgentLifecycleTaskProvider>
                </AiExecutionTaskProvider>
              </ConversationDataMaintenanceProvider>
            </ConversationSyncProvider>
          </ConversationCardKindRegistryProvider>
        </I18nProvider>
      </QueryScopeProvider>
    </QueryClientProvider>
  );
}
