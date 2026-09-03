import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  createTenant as createTenantRequest,
  getActiveTenant,
  listTenants,
  switchTenant as switchTenantRequest,
} from "../../services/tenants";
import type { Tenant, TenantCreateParams } from "../../types";
import type { QueryScope } from "./catalogQueries";

export interface QueryScopeContextValue {
  scope: QueryScope | null;
  activeTenant: Tenant | null;
  tenants: Tenant[];
  loading: boolean;
  tenantBusy: boolean;
  error: string | null;
  switchActiveTenant: (tenantId: string) => Promise<Tenant>;
  createLocalTenant: (params: TenantCreateParams) => Promise<Tenant>;
  reloadTenants: (preferredActiveTenant?: Tenant) => Promise<void>;
}

export const QueryScopeContext = createContext<QueryScopeContextValue | null>(
  null,
);

export const tenantKeys = {
  all: ["tenants"] as const,
  list: () => ["tenants", "list"] as const,
  active: () => ["tenants", "active"] as const,
};

export function QueryScopeProvider({
  children,
}: {
  children: ReactNode;
}): ReactNode {
  const queryClient = useQueryClient();
  const [epoch, setEpoch] = useState(1);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const activeTenantQuery = useQuery({
    queryKey: tenantKeys.active(),
    queryFn: getActiveTenant,
    networkMode: "always",
    retry: false,
    staleTime: 1000 * 60,
  });

  const tenantsListQuery = useQuery({
    queryKey: tenantKeys.list(),
    queryFn: listTenants,
    networkMode: "always",
    retry: false,
    staleTime: 1000 * 60,
  });

  const activeTenant = activeTenantQuery.data ?? null;

  const tenants = useMemo(() => {
    const rawList = tenantsListQuery.data ?? [];
    return activeTenant ? ensureTenantInList(rawList, activeTenant) : rawList;
  }, [tenantsListQuery.data, activeTenant]);

  const scope = useMemo<QueryScope | null>(() => {
    if (!activeTenant) return null;
    return { tenantId: activeTenant.id, epoch };
  }, [activeTenant, epoch]);

  const switchActiveTenant = useCallback(
    async (tenantId: string) => {
      if (activeTenant?.id === tenantId) {
        return activeTenant;
      }
      setBusy(true);
      const prevTenant = activeTenant;
      try {
        if (prevTenant) {
          await queryClient.cancelQueries({
            queryKey: ["tenant", prevTenant.id],
          });
        }
        const switched = await switchTenantRequest(tenantId);
        setEpoch((prev) => prev + 1);
        queryClient.setQueryData(tenantKeys.active(), switched);
        queryClient.setQueryData<Tenant[]>(tenantKeys.list(), (old) =>
          ensureTenantInList(old ?? [], switched),
        );
        setError(null);
        return switched;
      } catch (err) {
        setEpoch((prev) => prev + 1);
        if (prevTenant) {
          queryClient.setQueryData(tenantKeys.active(), prevTenant);
        }
        const message = err instanceof Error ? err.message : String(err);
        setError(message);
        throw err;
      } finally {
        setBusy(false);
      }
    },
    [activeTenant, queryClient],
  );

  const createLocalTenant = useCallback(
    async (params: TenantCreateParams) => {
      setBusy(true);
      try {
        const setActive = params.set_active ?? true;
        const tenant = await createTenantRequest({
          ...params,
          set_active: setActive,
        });
        queryClient.setQueryData<Tenant[]>(tenantKeys.list(), (old) =>
          ensureTenantInList(old ?? [], tenant),
        );
        if (setActive) {
          if (activeTenant) {
            await queryClient.cancelQueries({
              queryKey: ["tenant", activeTenant.id],
            });
          }
          setEpoch((prev) => prev + 1);
          queryClient.setQueryData(tenantKeys.active(), tenant);
        } else {
          await queryClient.invalidateQueries({ queryKey: tenantKeys.list() });
        }
        setError(null);
        return tenant;
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setError(message);
        throw err;
      } finally {
        setBusy(false);
      }
    },
    [activeTenant, queryClient],
  );

  const reloadTenants = useCallback(
    async (preferredActiveTenant?: Tenant) => {
      try {
        const [list, active] = await Promise.all([
          queryClient.fetchQuery({
            queryKey: tenantKeys.list(),
            queryFn: listTenants,
          }),
          queryClient.fetchQuery({
            queryKey: tenantKeys.active(),
            queryFn: getActiveTenant,
          }),
        ]);
        const nextActive = preferredActiveTenant ?? active;
        queryClient.setQueryData(tenantKeys.active(), nextActive);
        queryClient.setQueryData(
          tenantKeys.list(),
          ensureTenantInList(list, nextActive),
        );
        setError(null);
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setError(message);
        throw err;
      }
    },
    [queryClient],
  );

  const loading = activeTenantQuery.isLoading || tenantsListQuery.isLoading;

  const value = useMemo<QueryScopeContextValue>(
    () => ({
      scope,
      activeTenant,
      tenants,
      loading,
      tenantBusy: busy,
      error:
        error ??
        (activeTenantQuery.error ? String(activeTenantQuery.error) : null),
      switchActiveTenant,
      createLocalTenant,
      reloadTenants,
    }),
    [
      scope,
      activeTenant,
      tenants,
      loading,
      busy,
      error,
      activeTenantQuery.error,
      switchActiveTenant,
      createLocalTenant,
      reloadTenants,
    ],
  );

  return (
    <QueryScopeContext.Provider value={value}>
      {children}
    </QueryScopeContext.Provider>
  );
}

export function useQueryScope(): QueryScope | null {
  const context = useContext(QueryScopeContext);
  return context ? context.scope : null;
}

function ensureTenantInList(tenants: Tenant[], tenant: Tenant) {
  return [
    ...tenants.filter((candidate) => candidate.id !== tenant.id),
    tenant,
  ].sort((left, right) => left.name.localeCompare(right.name));
}
