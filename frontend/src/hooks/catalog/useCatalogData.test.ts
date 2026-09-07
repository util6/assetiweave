/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { createElement, type ReactNode } from "react";
import { act, renderHook } from "@testing-library/react";

import { beforeEach, describe, expect, it, vi } from "vitest";
import { QueryScopeProvider } from "../../app/query/QueryScopeProvider";
import { catalogKeys } from "../../app/query/catalogQueries";
import { fallbackNavigationModel } from "../../mock/catalog";
import { useCatalogData } from "./useCatalogData";

const updateNavigationModelMock = vi.hoisted(() => vi.fn());

vi.mock("../../services/tenants", () => ({
  getActiveTenant: vi.fn(async () => ({
    id: "default",
    name: "Default Workspace",
    slug: "default",
    kind: "local_workspace",
    status: "active",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
  })),
  listTenants: vi.fn(async () => []),
  createTenant: vi.fn(),
  switchTenant: vi.fn(),
}));

vi.mock("../../services/catalog", () => ({
  getNavigationModel: vi.fn(async () => ({
    activeRailId: "catalog",
    activeHeaderTabId: "skills",
    activeSubNavId: "overview",
    railItems: [],
    headerTabs: [
      { id: "skills", label: "Skills", assetKind: "skill", enabled: true },
    ],
    subNavItems: {
      skills: [
        {
          id: "overview",
          label: "Overview",
          routeKey: "skills.overview",
          enabled: true,
        },
      ],
    },
  })),
  getOverview: vi.fn(async () => null),
  listAppShortcutSettings: vi.fn(async () => []),
  listAssetMountStatuses: vi.fn(async () => []),
  listAssets: vi.fn(async () => []),
  listProfiles: vi.fn(async () => []),
  listSources: vi.fn(async () => []),
  refreshAssetMountStatuses: vi.fn(async () => []),
  updateAppShortcuts: vi.fn(async (shortcuts) => shortcuts),
  updateNavigationModel: updateNavigationModelMock,
}));

describe("useCatalogData navigation persistence", () => {
  let queryClient: QueryClient;

  beforeEach(() => {
    vi.useFakeTimers();
    queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    queryClient.setQueryData(["tenants", "active"], {
      id: "default",
      name: "Default Workspace",
      slug: "default",
      kind: "local_workspace",
      status: "active",
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
    });
    updateNavigationModelMock
      .mockReset()
      .mockImplementation(async (model) => model);
  });

  function wrapper({ children }: { children: ReactNode }) {
    return createElement(
      QueryClientProvider,
      { client: queryClient },
      createElement(QueryScopeProvider, null, children),
    );
  }

  it("useCatalogData 不包含数据实体状态 useState", async () => {
    const fs = await import("node:fs");
    const path = await import("node:path");
    const source = fs.readFileSync(
      path.resolve(__dirname, "./useCatalogData.ts"),
      "utf-8",
    );
    expect(source).not.toMatch(/useState<Asset\[\]>/);
    expect(source).not.toMatch(/useState<Source\[\]>/);
    expect(source).not.toMatch(/useState<TargetProfile\[\]>/);
    expect(source).not.toMatch(/useState<AppShortcut\[\]>/);
    expect(source).not.toMatch(/useState<AssetMountStatus\[\]>/);
    expect(source).not.toMatch(/useState<AppOverview/);
  });

  it("updates navigation immediately and persists the latest model after a short buffer", async () => {
    const { result } = renderHook(() => useCatalogData(), { wrapper });
    const nextModel = {
      ...result.current.navigationModel,
      activeSubNavId: "groups",
    };

    act(() => {
      result.current.deferNavigationModelSave(nextModel);
    });

    expect(result.current.navigationModel.activeSubNavId).toBe("groups");
    expect(updateNavigationModelMock).not.toHaveBeenCalled();

    await act(async () => {
      vi.advanceTimersByTime(120);
    });

    expect(updateNavigationModelMock).toHaveBeenCalledWith(nextModel);
  });

  it("coalesces rapid navigation changes into one persistence request", async () => {
    const { result } = renderHook(() => useCatalogData(), { wrapper });
    const firstModel = {
      ...result.current.navigationModel,
      activeSubNavId: "groups",
    };
    const secondModel = {
      ...result.current.navigationModel,
      activeSubNavId: "sources",
    };

    act(() => {
      result.current.deferNavigationModelSave(firstModel);
      result.current.deferNavigationModelSave(secondModel);
    });
    await act(async () => {
      vi.advanceTimersByTime(120);
    });

    expect(updateNavigationModelMock).toHaveBeenCalledTimes(1);
    expect(updateNavigationModelMock).toHaveBeenCalledWith(secondModel);
  });

  it("clears optimistic navigation once save settles to avoid masking query updates", async () => {
    const { result } = renderHook(() => useCatalogData(), { wrapper });
    const nextModel = {
      ...result.current.navigationModel,
      activeSubNavId: "groups",
    };

    act(() => {
      result.current.deferNavigationModelSave(nextModel);
    });
    expect(result.current.navigationModel.activeSubNavId).toBe("groups");

    await act(async () => {
      await vi.runAllTimersAsync();
    });

    // 保存完成后，query 数据更新能正常呈现，而不会被乐观状态遮蔽
    const updatedQueryModel = {
      ...nextModel,
      activeSubNavId: "conversations",
    };
    await act(async () => {
      queryClient.setQueryData(
        catalogKeys.navigation({ tenantId: "default", epoch: 1 }),
        updatedQueryModel,
      );
      await vi.runAllTimersAsync();
    });

    expect(result.current.navigationModel.activeSubNavId).toBe("conversations");
  });

  it("resets optimistic navigation immediately when active tenant changes", async () => {
    const { result } = renderHook(() => useCatalogData(), { wrapper });
    const tenantAModel = {
      ...result.current.navigationModel,
      activeSubNavId: "conversations",
    };

    act(() => {
      result.current.deferNavigationModelSave(tenantAModel);
    });
    expect(result.current.navigationModel.activeSubNavId).toBe("conversations");

    // 租户切换到 tenant-b
    const tenantBModel = {
      ...fallbackNavigationModel,
      activeSubNavId: "team",
    };
    queryClient.setQueryData(
      catalogKeys.navigation({ tenantId: "tenant-b", epoch: 1 }),
      tenantBModel,
    );

    await act(async () => {
      queryClient.setQueryData(["tenants", "active"], {
        id: "tenant-b",
        name: "Tenant B Workspace",
        slug: "tenant-b",
        kind: "local_workspace",
        status: "active",
        created_at: "2026-01-01T00:00:00Z",
        updated_at: "2026-01-01T00:00:00Z",
      });
      await vi.runAllTimersAsync();
    });

    // 租户切换后，乐观导航应被清空，并正确呈现新租户 B 的导航缓存 "team"
    expect(result.current.navigationModel.activeSubNavId).toBe("team");
  });
});
