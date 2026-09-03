/** @vitest-environment jsdom */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { act, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import {
  appSettingsKey,
  settingsQueryOptions,
  useSaveAppSettings,
} from "./settingsQueries";
import { useAppSettings } from "./useAppSettings";
import { defaultSettings } from "./settingsSchema";

const mockGetAppSettings = vi.hoisted(() =>
  vi.fn(async () => ({
    config_dir: "/tmp/app",
    config_path: "/tmp/app/data.db",
    conversation_adapter_dir: "/tmp/app/adapters",
    settings: { theme: "sunlight" } as Record<string, unknown>,
  })),
);

const mockSaveAppSettings = vi.hoisted(() =>
  vi.fn(async (settings: unknown) => ({
    config_dir: "/tmp/app",
    config_path: "/tmp/app/data.db",
    conversation_adapter_dir: "/tmp/app/adapters",
    settings,
  })),
);

vi.mock("../../services/appSettings", () => ({
  getAppSettings: mockGetAppSettings,
  saveAppSettings: mockSaveAppSettings,
}));

function createTestQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
}

function createWrapper(client: QueryClient) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
  };
}

describe("settingsQueries & useAppSettings", () => {
  it("后端设置替换启动缓存，不按租户复制", async () => {
    const client = createTestQueryClient();
    client.setQueryData(appSettingsKey, {
      settings: { theme: "old-cache" },
    });
    const result = await client.fetchQuery({
      ...settingsQueryOptions(),
      staleTime: 0,
    });
    expect(result.settings).toEqual({ theme: "sunlight" });
    expect(appSettingsKey).toEqual(["app-settings"]);
    client.clear();
  });

  it("跨组件双实例并发保存保证草稿合并与串行执行", async () => {
    const client = createTestQueryClient();
    client.setQueryData(appSettingsKey, {
      settings: {
        ...defaultSettings,
        theme: "promptStudio",
        density: "comfortable",
      },
    });

    const wrapper = createWrapper(client);
    const { result: instanceA } = renderHook(() => useAppSettings(), {
      wrapper,
    });
    const { result: instanceB } = renderHook(() => useAppSettings(), {
      wrapper,
    });

    await act(async () => {
      instanceA.current.updateSetting("theme", "midnight");
      instanceB.current.updateSetting("density", "compact");
      while (client.isMutating() > 0) {
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
    });

    // Both instances see the latest merged draft
    expect(instanceA.current.settings.theme).toBe("midnight");
    expect(instanceA.current.settings.density).toBe("compact");
    expect(instanceB.current.settings.theme).toBe("midnight");
    expect(instanceB.current.settings.density).toBe("compact");

    expect(mockSaveAppSettings).toHaveBeenCalled();
  });

  it("保存失败时保留草稿与错误状态，重试成功后恢复", async () => {
    const client = createTestQueryClient();
    client.setQueryData(appSettingsKey, {
      settings: { ...defaultSettings, theme: "promptStudio" },
    });

    mockSaveAppSettings.mockRejectedValueOnce(new Error("disk write failed"));

    const wrapper = createWrapper(client);
    const { result } = renderHook(() => useAppSettings(), { wrapper });

    await act(async () => {
      result.current.updateSetting("theme", "sunlight");
      while (client.isMutating() > 0) {
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
    });

    expect(result.current.settings.theme).toBe("sunlight");
    expect(result.current.settingsError).toBe("disk write failed");

    // Retry save succeeds
    mockSaveAppSettings.mockResolvedValueOnce({
      config_dir: "/tmp/app",
      config_path: "/tmp/app/data.db",
      conversation_adapter_dir: "/tmp/app/adapters",
      settings: { ...defaultSettings, theme: "sunlight" },
    });

    await act(async () => {
      result.current.retrySave();
      while (client.isMutating() > 0) {
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
    });

    expect(result.current.settings.theme).toBe("sunlight");
    expect(result.current.settingsError).toBeNull();
  });

  it("resetSettings 保留当前的 locale", async () => {
    const client = createTestQueryClient();
    client.setQueryData(appSettingsKey, {
      settings: { ...defaultSettings, theme: "midnight", locale: "zh" },
    });

    const wrapper = createWrapper(client);
    const { result } = renderHook(() => useAppSettings(), { wrapper });

    await act(async () => {
      result.current.resetSettings();
      while (client.isMutating() > 0) {
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
    });

    expect(result.current.settings.theme).toBe(defaultSettings.theme);
    expect(result.current.settings.locale).toBe("zh");
  });

  it("setColumnLayout 合并列宽偏好并持久化", async () => {
    mockGetAppSettings.mockResolvedValueOnce({
      config_dir: "/tmp/app",
      config_path: "/tmp/app/data.db",
      conversation_adapter_dir: "/tmp/app/adapters",
      settings: { ...defaultSettings, columnLayouts: { explorer: [1, 2] } },
    });
    const client = createTestQueryClient();
    client.setQueryData(appSettingsKey, {
      settings: { ...defaultSettings, columnLayouts: { explorer: [1, 2] } },
    });

    const wrapper = createWrapper(client);
    const { result } = renderHook(() => useAppSettings(), { wrapper });

    await act(async () => {
      await result.current.setColumnLayoutAsync("catalog", [1, 2, 1]);
      while (client.isMutating() > 0) {
        await new Promise((resolve) => setTimeout(resolve, 10));
      }
    });

    expect(result.current.settings.columnLayouts).toEqual({
      explorer: [1, 2],
      catalog: [1, 2, 1],
    });
  });
});
