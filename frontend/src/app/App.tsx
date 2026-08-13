import { AppRouter } from "../router/AppRouter";
import { AppClosePrompt } from "./AppClosePrompt";
import { AiExecutionTaskIndicator } from "./backgroundTasks/AiExecutionTaskIndicator";

export function App() {
  return (
    <>
      <AppRouter />
      <AiExecutionTaskIndicator />
      <AppClosePrompt />
    </>
  );
}
