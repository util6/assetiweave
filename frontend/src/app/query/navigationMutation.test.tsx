/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useSaveNavigation } from "./catalogMutations";
import { catalogKeys } from "./catalogQueries";
import type { NavigationModel } from "../../router/types";

const updateNavigationModelMock = vi.hoisted(() => vi.fn());

vi.mock("../../services/catalog", () => ({
  updateNavigationModel: updateNavigationModelMock,
  updateAssetDescription: vi.fn(),
  updateAppShortcuts: vi.fn(),
}));

describe("useSaveNavigation", () => {
  let queryClient: QueryClient;
  const scope = { tenantId: "workspace-a", epoch: 1 };

  const initialModel: NavigationModel = {
    activeRailId: "catalog",
    activeHeaderTabId: "skills",
    activeSubNavId: "overview",
    railItems: [],
    headerTabs: [{ id: "skills", label: "Skills", enabled: true }],
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
  };

  beforeEach(() => {
    vi.useFakeTimers();
    queryClient = new QueryClient({
      defaultOptions: { mutations: { retry: false } },
    });
    updateNavigationModelMock.mockReset();
    updateNavigationModelMock.mockImplementation(async (model) => model);
    queryClient.setQueryData(catalogKeys.navigation(scope), initialModel);
  });

  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
    queryClient.clear();
  });

  function wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );
  }

  it("120ms 内的多次 schedule 会合并为单次持久化请求并立即更新 UI 缓存", async () => {
    const { result } = renderHook(() => useSaveNavigation(scope), { wrapper });

    const model1: NavigationModel = {
      ...initialModel,
      activeSubNavId: "groups",
    };
    const model2: NavigationModel = {
      ...initialModel,
      activeSubNavId: "sources",
    };

    act(() => {
      result.current.schedule(model1);
    });

    // 立即更新 UI 缓存
    expect(
      queryClient.getQueryData<NavigationModel>(catalogKeys.navigation(scope))
        ?.activeSubNavId,
    ).toBe("groups");
    expect(updateNavigationModelMock).not.toHaveBeenCalled();

    act(() => {
      result.current.schedule(model2);
    });

    expect(
      queryClient.getQueryData<NavigationModel>(catalogKeys.navigation(scope))
        ?.activeSubNavId,
    ).toBe("sources");
    expect(updateNavigationModelMock).not.toHaveBeenCalled();

    await act(async () => {
      vi.advanceTimersByTime(120);
    });

    expect(updateNavigationModelMock).toHaveBeenCalledTimes(1);
    expect(updateNavigationModelMock).toHaveBeenCalledWith(model2);
  });

  it("旧导航 save 晚到不覆盖新选择", async () => {
    let resolveFirstSave: (model: NavigationModel) => void = () => {};
    const firstSavePromise = new Promise<NavigationModel>((resolve) => {
      resolveFirstSave = resolve;
    });

    updateNavigationModelMock
      .mockImplementationOnce(() => firstSavePromise)
      .mockImplementationOnce(async (model) => model);

    const { result } = renderHook(() => useSaveNavigation(scope), { wrapper });

    const olderModel: NavigationModel = {
      ...initialModel,
      activeSubNavId: "older-selection",
    };
    const newerModel: NavigationModel = {
      ...initialModel,
      activeSubNavId: "newer-selection",
    };

    // 启动第一个较慢的 save
    let olderPromise!: Promise<NavigationModel>;
    act(() => {
      olderPromise = result.current.save(olderModel);
    });

    // 在第一个响应返回前，用户进行了新的选择 schedule
    act(() => {
      result.current.schedule(newerModel);
    });

    // 此时缓存已经是 newerModel
    expect(
      queryClient.getQueryData<NavigationModel>(catalogKeys.navigation(scope))
        ?.activeSubNavId,
    ).toBe("newer-selection");

    // 第一个较慢的 save 迟到返回
    await act(async () => {
      resolveFirstSave(olderModel);
      await olderPromise;
    });

    // 缓存绝不应被旧响应回退
    expect(
      queryClient.getQueryData<NavigationModel>(catalogKeys.navigation(scope))
        ?.activeSubNavId,
    ).toBe("newer-selection");
  });

  it("组件卸载时清除尚未提交的 timer，防止卸载后触发未决 mutation", async () => {
    const { result, unmount } = renderHook(() => useSaveNavigation(scope), {
      wrapper,
    });

    const scheduledModel: NavigationModel = {
      ...initialModel,
      activeSubNavId: "groups",
    };

    act(() => {
      result.current.schedule(scheduledModel);
    });

    expect(updateNavigationModelMock).not.toHaveBeenCalled();

    // 卸载
    unmount();

    await act(async () => {
      vi.advanceTimersByTime(120);
    });

    // 卸载后不应再触发持久化请求
    expect(updateNavigationModelMock).not.toHaveBeenCalled();
  });
});
