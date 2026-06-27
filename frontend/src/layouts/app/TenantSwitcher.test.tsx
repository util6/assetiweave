/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ComponentProps } from "react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type { Tenant } from "../../types";
import { TenantSwitcher, TenantSwitcherDialog } from "./TenantSwitcher";

afterEach(cleanup);

describe("TenantSwitcher", () => {
  const defaultTenant = tenant("default", "Default Workspace");
  const clientTenant = tenant("client-a", "Client A");

  it("switches tenants from the selector", () => {
    const onSwitchTenant = vi.fn().mockResolvedValue(clientTenant);

    renderTenantManager({ onSwitchTenant });

    fireEvent.click(screen.getByRole("button", { name: "Tenants" }));
    fireEvent.change(within(screen.getByTestId("main-content-dialog-root")).getByRole("combobox", { name: "Switch tenant" }), {
      target: { value: "client-a" },
    });

    expect(onSwitchTenant).toHaveBeenCalledWith("client-a");
  });

  it("creates an active tenant from the dialog", async () => {
    const onCreateTenant = vi.fn().mockResolvedValue(clientTenant);

    renderTenantManager({ onCreateTenant });
    fireEvent.click(screen.getByRole("button", { name: "Tenants" }));
    fireEvent.change(screen.getByLabelText("Tenant name *"), { target: { value: "Client A" } });
    fireEvent.change(screen.getByLabelText("Tenant slug"), { target: { value: "client-a" } });
    fireEvent.click(screen.getByRole("button", { name: "Create tenant" }));

    await waitFor(() =>
      expect(onCreateTenant).toHaveBeenCalledWith({
        name: "Client A",
        set_active: true,
        slug: "client-a",
      }),
    );
  });

  it("renders the tenant dialog outside the side rail trigger container", () => {
    renderTenantManager();

    fireEvent.click(within(screen.getByTestId("side-rail-action")).getByRole("button", { name: "Tenants" }));

    expect(within(screen.getByTestId("side-rail-action")).queryByRole("dialog")).toBeNull();
    expect(within(screen.getByTestId("main-content-dialog-root")).getByRole("dialog")).not.toBeNull();
  });

  it("uses the side rail trigger as the permanent entry point", () => {
    const { rerender } = renderTenantSwitcher();

    expect(screen.getByRole("button", { name: "Tenants" }).getAttribute("title")).toBe("Tenants: Default Workspace");
    expect(screen.queryByText("Default Workspace")).toBeNull();

    rerender(
      <I18nProvider>
          <TenantSwitcher
            activeTenant={defaultTenant}
            busy={false}
            expanded
            loading={false}
            onOpen={vi.fn()}
          open={false}
        />
      </I18nProvider>,
    );

    expect(screen.getByRole("button", { name: "Tenants" })).not.toBeNull();
    expect(screen.getByText("Default Workspace")).not.toBeNull();
  });

  function renderTenantSwitcher(
    overrides: Partial<ComponentProps<typeof TenantSwitcher>> = {},
  ) {
    return render(
      <I18nProvider>
        <TenantSwitcher
          activeTenant={defaultTenant}
          busy={false}
          loading={false}
          onOpen={vi.fn()}
          open={false}
          {...overrides}
        />
      </I18nProvider>,
    );
  }

  function renderTenantManager(
    overrides: Partial<ComponentProps<typeof TenantSwitcherDialog>> = {},
  ) {
    function Fixture() {
      const [open, setOpen] = useState(false);
      return (
        <I18nProvider>
          <aside data-testid="side-rail-action">
            <TenantSwitcher
              activeTenant={defaultTenant}
              busy={false}
              loading={false}
              onOpen={() => setOpen(true)}
              open={open}
            />
          </aside>
          <main data-testid="main-content-dialog-root">
            {open ? (
              <TenantSwitcherDialog
                activeTenant={defaultTenant}
                busy={false}
                error={null}
                loading={false}
                onClose={() => setOpen(false)}
                onCreateTenant={vi.fn().mockResolvedValue(defaultTenant)}
                onSwitchTenant={vi.fn().mockResolvedValue(defaultTenant)}
                tenants={[defaultTenant, clientTenant]}
                {...overrides}
              />
            ) : null}
          </main>
        </I18nProvider>
      );
    }

    return render(<Fixture />);
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
