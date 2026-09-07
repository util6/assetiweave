import { useContext } from "react";
import { QueryScopeContext } from "../../app/query/QueryScopeProvider";
import type { Tenant, TenantCreateParams } from "../../types";

export interface TenantControllerOptions {
  onTenantChanged?: (tenant: Tenant) => Promise<void> | void;
}

export function useTenantController({
  onTenantChanged,
}: TenantControllerOptions = {}) {
  const context = useContext(QueryScopeContext);
  if (!context) {
    throw new Error(
      "useTenantController must be used within a QueryScopeProvider",
    );
  }

  return {
    activeTenant: context.activeTenant,
    createLocalTenant: async (params: TenantCreateParams) => {
      const tenant = await context.createLocalTenant(params);
      if (params.set_active ?? true) {
        await onTenantChanged?.(tenant);
      }
      return tenant;
    },
    error: context.error,
    loading: context.loading,
    reloadTenants: context.reloadTenants,
    switchActiveTenant: async (tenantId: string) => {
      const tenant = await context.switchActiveTenant(tenantId);
      await onTenantChanged?.(tenant);
      return tenant;
    },
    tenantBusy: context.tenantBusy,
    tenants: context.tenants,
  };
}
