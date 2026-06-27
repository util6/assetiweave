import { invoke } from "@tauri-apps/api/core";
import type { Tenant, TenantCreateParams } from "../types";

const fallbackTenant: Tenant = {
  id: "default",
  slug: "default",
  name: "Default Workspace",
  kind: "local_workspace",
  status: "active",
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};

export async function listTenants(): Promise<Tenant[]> {
  try {
    return await invoke<Tenant[]>("list_tenants");
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }
    return [fallbackTenant];
  }
}

export async function getActiveTenant(): Promise<Tenant> {
  try {
    return await invoke<Tenant>("get_active_tenant");
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }
    return fallbackTenant;
  }
}

export async function createTenant(params: TenantCreateParams): Promise<Tenant> {
  const payload = {
    name: params.name.trim(),
    set_active: params.set_active ?? true,
    slug: params.slug?.trim() || null,
  };

  if (!payload.name) {
    throw new Error("Tenant name is required");
  }

  try {
    return await invoke<Tenant>("create_tenant", { params: payload });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }
    const slug = payload.slug ?? slugifyTenantName(payload.name);
    const timestamp = new Date().toISOString();
    return {
      id: slug,
      slug,
      name: payload.name,
      kind: "local_workspace",
      status: "active",
      created_at: timestamp,
      updated_at: timestamp,
    };
  }
}

export async function switchTenant(tenantId: string): Promise<Tenant> {
  const id = tenantId.trim();
  if (!id) {
    throw new Error("Tenant id is required");
  }

  try {
    return await invoke<Tenant>("switch_tenant", { tenantId: id });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }
    return {
      ...fallbackTenant,
      id,
      slug: id,
      name: id === fallbackTenant.id ? fallbackTenant.name : id,
      updated_at: new Date().toISOString(),
    };
  }
}

function slugifyTenantName(name: string) {
  return (
    name
      .toLowerCase()
      .trim()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "tenant"
  );
}

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
