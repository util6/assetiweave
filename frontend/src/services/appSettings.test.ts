import { describe, expect, it, vi } from "vitest";
import {
  getAppSettings,
  initializeAppLocaleIfUnset,
  saveAppSettings,
} from "./appSettings";
import { defaultSettings } from "../store/settings/settingsSchema";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";

const invokeMock = vi.mocked(invoke);

describe("appSettings service", () => {
  it("getAppSettings invokes get_app_settings", async () => {
    const mockResult = {
      config_dir: "/test/dir",
      config_path: "/test/config.json",
      conversation_adapter_dir: "/test/adapters",
      settings: defaultSettings,
    };
    invokeMock.mockResolvedValueOnce(mockResult);

    const result = await getAppSettings();
    expect(invokeMock).toHaveBeenCalledWith("get_app_settings");
    expect(result).toEqual(mockResult);
  });

  it("saveAppSettings invokes save_app_settings with payload", async () => {
    const mockResult = {
      config_dir: "/test/dir",
      config_path: "/test/config.json",
      conversation_adapter_dir: "/test/adapters",
      settings: defaultSettings,
    };
    invokeMock.mockResolvedValueOnce(mockResult);

    const result = await saveAppSettings(defaultSettings);
    expect(invokeMock).toHaveBeenCalledWith("save_app_settings", {
      settings: defaultSettings,
    });
    expect(result).toEqual(mockResult);
  });

  it("initializeAppLocaleIfUnset invokes initialize_app_locale_if_unset with locale payload", async () => {
    const mockResultZh = {
      config_dir: "/test/dir",
      config_path: "/test/config.json",
      conversation_adapter_dir: "/test/adapters",
      settings: { ...defaultSettings, locale: "zh" },
    };
    invokeMock.mockResolvedValueOnce(mockResultZh);

    const resultZh = await initializeAppLocaleIfUnset("zh");
    expect(invokeMock).toHaveBeenCalledWith("initialize_app_locale_if_unset", {
      locale: "zh",
    });
    expect(resultZh).toEqual(mockResultZh);

    const mockResultEn = {
      config_dir: "/test/dir",
      config_path: "/test/config.json",
      conversation_adapter_dir: "/test/adapters",
      settings: { ...defaultSettings, locale: "en" },
    };
    invokeMock.mockResolvedValueOnce(mockResultEn);

    const resultEn = await initializeAppLocaleIfUnset("en");
    expect(invokeMock).toHaveBeenCalledWith("initialize_app_locale_if_unset", {
      locale: "en",
    });
    expect(resultEn).toEqual(mockResultEn);
  });
});
