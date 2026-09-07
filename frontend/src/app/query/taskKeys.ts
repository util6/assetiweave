import type { QueryScope } from "./catalogQueries";

export const taskKeys = {
  resource: (scope: QueryScope, domain: string) =>
    ["tenant", scope.tenantId, scope.epoch, "tasks", domain] as const,
};
