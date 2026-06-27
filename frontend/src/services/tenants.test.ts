import { beforeEach, describe, expect, it, vi } from "vitest";
import { createTenant, getActiveTenant, listTenants, switchTenant } from "./tenants";
import type { Tenant } from "../types";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

describe("tenant services", () => {
  const tenant: Tenant = {
    id: "client-a",
    slug: "client-a",
    name: "Client A",
    kind: "local_workspace",
    status: "active",
    created_at: "2026-06-27T00:00:00Z",
    updated_at: "2026-06-27T00:00:00Z",
  };

  beforeEach(() => {
    invokeMock.mockReset();
    if (typeof window !== "undefined") {
      Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
    }
  });

  it("reads the tenant list and active tenant from Tauri", async () => {
    invokeMock.mockImplementation(async (command: string) => (command === "list_tenants" ? [tenant] : tenant));

    await expect(listTenants()).resolves.toEqual([tenant]);
    await expect(getActiveTenant()).resolves.toEqual(tenant);

    expect(invokeMock).toHaveBeenNthCalledWith(1, "list_tenants");
    expect(invokeMock).toHaveBeenNthCalledWith(2, "get_active_tenant");
  });

  it("creates a tenant through the typed params payload", async () => {
    invokeMock.mockResolvedValue(tenant);

    await expect(createTenant({ name: " Client A ", slug: " client-a " })).resolves.toEqual(tenant);

    expect(invokeMock).toHaveBeenCalledWith("create_tenant", {
      params: {
        name: "Client A",
        set_active: true,
        slug: "client-a",
      },
    });
  });

  it("switches tenants using the Tauri camelCase argument name", async () => {
    invokeMock.mockResolvedValue(tenant);

    await expect(switchTenant(" client-a ")).resolves.toEqual(tenant);

    expect(invokeMock).toHaveBeenCalledWith("switch_tenant", { tenantId: "client-a" });
  });

  it("keeps browser previews usable without the Tauri runtime", async () => {
    invokeMock.mockRejectedValue(new Error("preview"));

    await expect(listTenants()).resolves.toMatchObject([{ id: "default", name: "Default Workspace" }]);
    await expect(getActiveTenant()).resolves.toMatchObject({ id: "default", name: "Default Workspace" });
    await expect(createTenant({ name: "Client Preview", set_active: true })).resolves.toMatchObject({
      id: "client-preview",
      name: "Client Preview",
    });
    await expect(switchTenant("client-preview")).resolves.toMatchObject({
      id: "client-preview",
      name: "client-preview",
    });
  });

  it("rejects empty create and switch inputs before invoking Tauri", async () => {
    await expect(createTenant({ name: " " })).rejects.toThrow("Tenant name is required");
    await expect(switchTenant(" ")).rejects.toThrow("Tenant id is required");

    expect(invokeMock).not.toHaveBeenCalled();
  });
});
