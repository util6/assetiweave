import { AppRouter } from "../router/AppRouter";
import { AppClosePrompt } from "./AppClosePrompt";
import { AiExecutionTaskIndicator } from "./backgroundTasks/AiExecutionTaskIndicator";
import { AgentLifecycleTaskIndicator } from "./backgroundTasks/AgentLifecycleTaskIndicator";
import { CatalogTaskIndicator } from "./backgroundTasks/CatalogTaskIndicator";
import { ConversationDataMaintenanceTaskIndicator } from "./backgroundTasks/ConversationDataMaintenanceTaskIndicator";

export function App() {
  return (
    <>
      <AppRouter />
      <AiExecutionTaskIndicator />
      <AgentLifecycleTaskIndicator />
      <CatalogTaskIndicator />
      <ConversationDataMaintenanceTaskIndicator />
      <AppClosePrompt />
    </>
  );
}
