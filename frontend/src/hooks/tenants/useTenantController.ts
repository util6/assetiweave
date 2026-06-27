import { useEffect, useState } from "react";
import {
  createTenant as createTenantRequest,
  getActiveTenant,
  listTenants,
  switchTenant as switchTenantRequest,
} from "../../services/tenants";
import type { Tenant, TenantCreateParams } from "../../types";

export interface TenantControllerOptions {
  onTenantChanged?: (tenant: Tenant) => Promise<void> | void;
}

export function useTenantController({ onTenantChanged }: TenantControllerOptions = {}) {
  const [activeTenant, setActiveTenant] = useState<Tenant | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [tenants, setTenants] = useState<Tenant[]>([]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void loadTenantState()
      .then(({ active, list }) => {
        if (cancelled) return;
        setActiveTenant(active);
        setTenants(ensureTenantInList(list, active));
        setError(null);
      })
      .catch((nextError) => {
        if (!cancelled) {
          setError(errorMessage(nextError));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  async function reloadTenants(preferredActiveTenant?: Tenant) {
    setLoading(true);
    try {
      const { active, list } = await loadTenantState();
      const nextActive = preferredActiveTenant ?? active;
      setActiveTenant(nextActive);
      setTenants(ensureTenantInList(list, nextActive));
      setError(null);
    } catch (nextError) {
      setError(errorMessage(nextError));
      throw nextError;
    } finally {
      setLoading(false);
    }
  }

  async function createLocalTenant(params: TenantCreateParams) {
    setBusy(true);
    try {
      const setActive = params.set_active ?? true;
      const tenant = await createTenantRequest({ ...params, set_active: setActive });
      setTenants((current) => ensureTenantInList(current, tenant));
      if (setActive) {
        setActiveTenant(tenant);
        await onTenantChanged?.(tenant);
      } else {
        await reloadTenants();
      }
      setError(null);
      return tenant;
    } catch (nextError) {
      setError(errorMessage(nextError));
      throw nextError;
    } finally {
      setBusy(false);
    }
  }

  async function switchActiveTenant(tenantId: string) {
    if (activeTenant?.id === tenantId) {
      return activeTenant;
    }

    setBusy(true);
    try {
      const tenant = await switchTenantRequest(tenantId);
      setActiveTenant(tenant);
      setTenants((current) => ensureTenantInList(current, tenant));
      await onTenantChanged?.(tenant);
      setError(null);
      return tenant;
    } catch (nextError) {
      setError(errorMessage(nextError));
      throw nextError;
    } finally {
      setBusy(false);
    }
  }

  return {
    activeTenant,
    createLocalTenant,
    error,
    loading,
    reloadTenants,
    switchActiveTenant,
    tenantBusy: busy,
    tenants,
  };
}

async function loadTenantState() {
  const [list, active] = await Promise.all([listTenants(), getActiveTenant()]);
  return { active, list };
}

function ensureTenantInList(tenants: Tenant[], tenant: Tenant) {
  return [...tenants.filter((candidate) => candidate.id !== tenant.id), tenant].sort((left, right) =>
    left.name.localeCompare(right.name),
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
