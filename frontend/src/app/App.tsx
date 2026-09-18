import { AppRouter } from "../router/AppRouter";
import { AppClosePrompt } from "./AppClosePrompt";

export function App() {
  return (
    <>
      <AppRouter />
      <AppClosePrompt />
    </>
  );
}
