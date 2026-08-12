/* @vitest-environment jsdom */

import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useCatalogData } from "./useCatalogData";

const updateNavigationModelMock = vi.hoisted(() => vi.fn());

vi.mock("../../services/catalog", () => ({
  getNavigationModel: vi.fn(async () => ({
    activeRailId: "catalog",
    activeHeaderTabId: "skills",
    activeSubNavId: "overview",
    railItems: [],
    headerTabs: [{ id: "skills", label: "Skills", assetKind: "skill", enabled: true }],
    subNavItems: { skills: [{ id: "overview", label: "Overview", routeKey: "skills.overview", enabled: true }] },
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
  beforeEach(() => {
    vi.useFakeTimers();
    updateNavigationModelMock.mockReset().mockImplementation(async (model) => model);
  });

  it("updates navigation immediately and persists the latest model after a short buffer", async () => {
    const { result } = renderHook(() => useCatalogData());
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
    const { result } = renderHook(() => useCatalogData());
    const firstModel = { ...result.current.navigationModel, activeSubNavId: "groups" };
    const secondModel = { ...result.current.navigationModel, activeSubNavId: "sources" };

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
