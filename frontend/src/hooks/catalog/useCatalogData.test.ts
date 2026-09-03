/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { createElement, type ReactNode } from "react";
import { act, renderHook } from "@testing-library/react";

import { beforeEach, describe, expect, it, vi } from "vitest";
import { QueryScopeProvider } from "../../app/query/QueryScopeProvider";
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
});
