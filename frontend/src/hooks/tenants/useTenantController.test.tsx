/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useTenantController } from "./useTenantController";
import type { Tenant } from "../../types";

const listTenantsMock = vi.hoisted(() => vi.fn());
const getActiveTenantMock = vi.hoisted(() => vi.fn());
const createTenantMock = vi.hoisted(() => vi.fn());
const switchTenantMock = vi.hoisted(() => vi.fn());

vi.mock("../../services/tenants", () => ({
  createTenant: createTenantMock,
  getActiveTenant: getActiveTenantMock,
  listTenants: listTenantsMock,
  switchTenant: switchTenantMock,
}));

afterEach(cleanup);

describe("useTenantController", () => {
  const defaultTenant = tenant("default", "Default Workspace");
  const clientTenant = tenant("client-a", "Client A");

  beforeEach(() => {
    listTenantsMock.mockReset();
    getActiveTenantMock.mockReset();
    createTenantMock.mockReset();
    switchTenantMock.mockReset();
    listTenantsMock.mockResolvedValue([defaultTenant, clientTenant]);
    getActiveTenantMock.mockResolvedValue(defaultTenant);
  });

  it("loads tenants and the active tenant", async () => {
    render(<Fixture />);

    await screen.findByText("Default Workspace");
    expect(screen.getByText("2 tenants")).toBeTruthy();
  });

  it("switches the active tenant and refreshes tenant-scoped data", async () => {
    const onTenantChanged = vi.fn();
    switchTenantMock.mockResolvedValue(clientTenant);

    render(<Fixture onTenantChanged={onTenantChanged} />);
    await screen.findByText("Default Workspace");
    fireEvent.click(screen.getByRole("button", { name: "Switch client" }));

    await waitFor(() => expect(onTenantChanged).toHaveBeenCalledWith(clientTenant));
    expect(screen.getByText("Client A")).toBeTruthy();
  });

  it("creates an active tenant and refreshes tenant-scoped data", async () => {
    const onTenantChanged = vi.fn();
    const createdTenant = tenant("new-client", "New Client");
    createTenantMock.mockResolvedValue(createdTenant);

    render(<Fixture onTenantChanged={onTenantChanged} />);
    await screen.findByText("Default Workspace");
    fireEvent.click(screen.getByRole("button", { name: "Create tenant" }));

    await waitFor(() => expect(onTenantChanged).toHaveBeenCalledWith(createdTenant));
    expect(screen.getByText("New Client")).toBeTruthy();
  });

  function Fixture({ onTenantChanged = vi.fn() }: { onTenantChanged?: (tenant: Tenant) => void }) {
    const tenants = useTenantController({ onTenantChanged });
    return (
      <div>
        <div>{tenants.activeTenant?.name ?? "No tenant"}</div>
        <div>{tenants.tenants.length} tenants</div>
        <button onClick={() => void tenants.switchActiveTenant("client-a")} type="button">
          Switch client
        </button>
        <button onClick={() => void tenants.createLocalTenant({ name: "New Client" })} type="button">
          Create tenant
        </button>
      </div>
    );
  }
});

function tenant(id: string, name: string): Tenant {
  return {
    id,
    slug: id,
    name,
    kind: "local_workspace",
    status: "active",
    created_at: "2026-06-27T00:00:00Z",
    updated_at: "2026-06-27T00:00:00Z",
  };
}
