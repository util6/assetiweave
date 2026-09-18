import { describe, expect, it } from "vitest";
import {
  getConversationUsageDashboard,
  getConversationUsageScanStatus,
  scanConversationUsage,
  isTauriRuntime,
} from "./usage";

describe("usage service in browser / mock mode", () => {
  it("detects non-tauri runtime correctly in node/vitest", () => {
    expect(isTauriRuntime()).toBe(false);
  });

  it("returns mock dashboard when outside tauri", async () => {
    const data = await getConversationUsageDashboard();
    expect(data).toBeDefined();
    expect(data.hero.totalTokens).toBeGreaterThan(0);
    expect(data.models.length).toBeGreaterThan(0);
    expect(data.sources.length).toBeGreaterThan(0);
    expect(data.dailyTrend.length).toBeGreaterThan(0);
    expect(data.dates.length).toBeGreaterThan(0);
    expect(data.hero.costsByCurrency.length).toBeGreaterThan(0);
  });

  it("returns mock scan status when outside tauri", async () => {
    const status = await getConversationUsageScanStatus();
    expect(status).toBeDefined();
    expect(status.scannedSourcesCount).toBeGreaterThan(0);
    expect(status.totalEventsCount).toBeGreaterThan(0);
  });

  it("returns scan status on scan trigger when outside tauri", async () => {
    const started = await scanConversationUsage({ mode: "full" });
    expect(started).toBeDefined();
    expect(started.activeScanTaskId).toBe("mock-task-usage-scan");
  });
});
