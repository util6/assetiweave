import { describe, expect, it, vi } from "vitest";
import { ensureAppLocale, resolveInitialLocale } from "./localeBootstrap";
import type { AppSettingsFile } from "../services/appSettings";

const initialize = vi.hoisted(() => vi.fn());
vi.mock("../services/appSettings", () => ({
  initializeAppLocaleIfUnset: initialize,
}));

describe("localeBootstrap", () => {
  describe("resolveInitialLocale", () => {
    it("returns stored valid locale when available", () => {
      expect(resolveInitialLocale("zh", "en-US")).toBe("zh");
      expect(resolveInitialLocale("en", "zh-CN")).toBe("en");
    });

    it("uses navigator language if stored is absent or invalid", () => {
      expect(resolveInitialLocale(null, "zh-CN")).toBe("zh");
      expect(resolveInitialLocale(null, "en-US")).toBe("en");
      expect(resolveInitialLocale("invalid", "zh-TW")).toBe("zh");
      expect(resolveInitialLocale("invalid", "fr-FR")).toBe("en");
    });

    it("defaults to zh when nothing is specified and no navigator language exists", () => {
      expect(resolveInitialLocale(null, "")).toBe("zh");
    });
  });

  describe("ensureAppLocale", () => {
    it("SQLite显式语言优先于旧localStorage", async () => {
      const file: AppSettingsFile = {
        config_dir: "/tmp/app",
        config_path: "/tmp/app/data.db",
        conversation_adapter_dir: "/tmp/app/adapters",
        settings: { locale: "en" },
      };
      const storage = { getItem: vi.fn(() => "zh"), removeItem: vi.fn() };
      initialize.mockClear();

      expect(await ensureAppLocale(file, storage, "zh-CN")).toBe(file);
      expect(initialize).not.toHaveBeenCalled();
      expect(storage.removeItem).toHaveBeenCalledWith("assetiweave.locale");
    });

    it("null语言分支：CAS返回的语言生效并删除旧键", async () => {
      const file: AppSettingsFile = {
        config_dir: "/tmp/app",
        config_path: "/tmp/app/data.db",
        conversation_adapter_dir: "/tmp/app/adapters",
        settings: { locale: null },
      };
      const winnerFile: AppSettingsFile = {
        ...file,
        settings: { locale: "en" },
      };
      initialize.mockReset();
      initialize.mockResolvedValueOnce(winnerFile);

      const storage = { getItem: vi.fn(() => "zh"), removeItem: vi.fn() };
      const result = await ensureAppLocale(file, storage, "en-US");

      expect(initialize).toHaveBeenCalledWith("zh");
      expect(result).toBe(winnerFile);
      expect(storage.removeItem).toHaveBeenCalledWith("assetiweave.locale");
    });

    it("初始化失败保留旧键，不写假成功标记", async () => {
      const file: AppSettingsFile = {
        config_dir: "/tmp/app",
        config_path: "/tmp/app/data.db",
        conversation_adapter_dir: "/tmp/app/adapters",
        settings: { locale: null },
      };
      initialize.mockReset();
      initialize.mockRejectedValueOnce(new Error("CAS failed"));

      const storage = { getItem: vi.fn(() => "zh"), removeItem: vi.fn() };
      await expect(ensureAppLocale(file, storage, "zh-CN")).rejects.toThrow(
        "CAS failed",
      );
      expect(storage.removeItem).not.toHaveBeenCalled();
    });

    it("storage访问抛异常时继续使用navigator，不使应用崩溃", async () => {
      const file: AppSettingsFile = {
        config_dir: "/tmp/app",
        config_path: "/tmp/app/data.db",
        conversation_adapter_dir: "/tmp/app/adapters",
        settings: { locale: null },
      };
      const updatedFile: AppSettingsFile = {
        ...file,
        settings: { locale: "en" },
      };
      initialize.mockReset();
      initialize.mockResolvedValueOnce(updatedFile);

      const storage = {
        getItem: vi.fn(() => {
          throw new Error("SecurityError: localStorage is disabled");
        }),
        removeItem: vi.fn(() => {
          throw new Error("SecurityError: localStorage is disabled");
        }),
      };

      const result = await ensureAppLocale(file, storage, "en-US");
      expect(initialize).toHaveBeenCalledWith("en");
      expect(result).toBe(updatedFile);
    });
  });
});
