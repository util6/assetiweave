/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { QueryScopeProvider, useQueryScope } from "./QueryScopeProvider";
import { useTenantController } from "../../hooks/tenants/useTenantController";
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

describe("QueryScopeProvider", () => {
  let queryClient: QueryClient;
  const tenantA: Tenant = {
    id: "tenant-a",
    slug: "tenant-a",
    name: "Tenant A",
    kind: "local_workspace",
    status: "active",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
  };
  const tenantB: Tenant = {
    id: "tenant-b",
    slug: "tenant-b",
    name: "Tenant B",
    kind: "local_workspace",
    status: "active",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
  };

  beforeEach(() => {
    queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    listTenantsMock.mockReset();
    getActiveTenantMock.mockReset();
    createTenantMock.mockReset();
    switchTenantMock.mockReset();

    listTenantsMock.mockResolvedValue([tenantA, tenantB]);
    getActiveTenantMock.mockResolvedValue(tenantA);
  });

  afterEach(() => {
    queryClient.clear();
  });

  function wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <QueryScopeProvider>{children}</QueryScopeProvider>
      </QueryClientProvider>
    );
  }

  it("初始状态提供 activeTenant 对应的 QueryScope", async () => {
    const { result } = renderHook(
      () => ({
        scope: useQueryScope(),
        controller: useTenantController(),
      }),
      { wrapper },
    );

    await waitFor(() => {
      expect(result.current.scope).toEqual({
        tenantId: "tenant-a",
        epoch: 1,
      });
    });
    expect(result.current.controller.activeTenant?.name).toBe("Tenant A");
  });

  it("切换租户递增 epoch 并发布新 scope，防止旧租户迟到结果覆盖新租户", async () => {
    switchTenantMock.mockResolvedValue(tenantB);

    const { result } = renderHook(
      () => ({
        scope: useQueryScope(),
        controller: useTenantController(),
      }),
      { wrapper },
    );

    await waitFor(() => {
      expect(result.current.scope?.tenantId).toBe("tenant-a");
    });

    await act(async () => {
      await result.current.controller.switchActiveTenant("tenant-b");
    });

    expect(result.current.scope).toEqual({
      tenantId: "tenant-b",
      epoch: 2,
    });
    expect(result.current.controller.activeTenant?.name).toBe("Tenant B");

    // 证明 tenant A 的迟到写入不会覆盖 tenant B
    const staleScopeA = { tenantId: "tenant-a", epoch: 1 };
    queryClient.setQueryData(
      [
        "tenant",
        staleScopeA.tenantId,
        staleScopeA.epoch,
        "catalog",
        "assets",
        "all",
      ],
      [{ id: "stale-asset" }],
    );

    const activeData = queryClient.getQueryData([
      "tenant",
      result.current.scope!.tenantId,
      result.current.scope!.epoch,
      "catalog",
      "assets",
      "all",
    ]);
    expect(activeData).toBeUndefined();
  });

  it("租户切换失败恢复旧租户并递增 epoch 重读", async () => {
    switchTenantMock.mockRejectedValue(new Error("switch failed"));

    const { result } = renderHook(
      () => ({
        scope: useQueryScope(),
        controller: useTenantController(),
      }),
      { wrapper },
    );

    await waitFor(() => {
      expect(result.current.scope?.tenantId).toBe("tenant-a");
    });

    await act(async () => {
      await expect(
        result.current.controller.switchActiveTenant("tenant-b"),
      ).rejects.toThrow("switch failed");
    });

    // 失败后恢复旧租户，但使用新 epoch (2) 重读，旧请求不会落到恢复后的新 epoch 上
    expect(result.current.scope).toEqual({
      tenantId: "tenant-a",
      epoch: 2,
    });
    expect(result.current.controller.error).toBe("switch failed");
  });
});
