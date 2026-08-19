import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultSettings } from "./settingsSchema";
import { readCachedSettings, writeCachedSettings } from "./settingsPersistence";

describe("settings persistence contract", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("uses the cache for bootstrap and round-trips normalized settings", () => {
    const storage = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => storage.set(key, value),
    });

    writeCachedSettings({ ...defaultSettings, density: "compact" });

    expect(readCachedSettings().density).toBe("compact");
  });

  it("falls back to defaults when the cache is malformed", () => {
    vi.stubGlobal("localStorage", { getItem: () => "{invalid", setItem: () => undefined });
    expect(readCachedSettings()).toEqual(defaultSettings);
  });
});
