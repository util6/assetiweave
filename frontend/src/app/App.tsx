import { AppRouter } from "../router/AppRouter";
import { AppClosePrompt } from "./AppClosePrompt";
import { AiExecutionTaskIndicator } from "./backgroundTasks/AiExecutionTaskIndicator";
import { AgentLifecycleTaskIndicator } from "./backgroundTasks/AgentLifecycleTaskIndicator";

export function App() {
  return (
    <>
      <AppRouter />
      <AiExecutionTaskIndicator />
      <AgentLifecycleTaskIndicator />
      <AppClosePrompt />
    </>
  );
}
