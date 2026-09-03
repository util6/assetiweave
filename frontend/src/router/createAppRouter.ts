import { createMemoryHistory, createRouter } from "@tanstack/react-router";
import { routeTree } from "./routeTree";

export function createAppRouter(initialPath = "/skills/overview") {
  const history = createMemoryHistory({ initialEntries: [initialPath] });
  return createRouter({
    routeTree,
    history,
  });
}

export type AppRouterInstance = ReturnType<typeof createAppRouter>;
