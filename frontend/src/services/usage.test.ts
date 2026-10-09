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

  it("returns mock scan status on scan trigger when outside tauri", async () => {
    const started = await scanConversationUsage({ mode: "full" });
    expect(started).toBeDefined();
    expect(started.activeScanTaskId).toBe("mock-task-usage-scan");
  });

  it("returns mock model pricing catalog outside tauri", async () => {
    const catalog = await (await import("./usage")).getModelPricingCatalog();
    expect(catalog).toBeDefined();
    expect(catalog.models.length).toBeGreaterThan(0);
    expect(catalog.usdToCnyRate).toBeGreaterThan(6.0);
    expect(catalog.activeChannel).toBeDefined();
    const gpt = catalog.models.find((m) =>
      m.modelPatterns.some((p) => p.includes("gpt-4o")),
    );
    expect(gpt).toBeDefined();
  });

  it("returns mock pricing status and triggers pricing update task outside tauri", async () => {
    const { getModelPricingStatus, updateModelPricingCatalog } =
      await import("./usage");
    const status = await getModelPricingStatus();
    expect(status).toBeDefined();
    expect(status?.modelsCount).toBeGreaterThan(0);

    const snapshot = await updateModelPricingCatalog();
    expect(snapshot).toBeDefined();
    expect(snapshot.id).toBeDefined();
    expect(snapshot.progress.modelsCount).toBeGreaterThan(0);

    const selectiveSnapshot = await updateModelPricingCatalog(false, ["gpt-5.6-luna", "gemini-3.8-flash"]);
    expect(selectiveSnapshot.progress.modelsCount).toBe(2);
  });

  it("supports setModelPricingExchangeRate and saveModelPriceEntry outside tauri", async () => {
    const { setModelPricingExchangeRate, saveModelPriceEntry } =
      await import("./usage");
    const updated = await setModelPricingExchangeRate(7.35);
    expect(updated).toBeDefined();
    expect(updated.exchangeRate).toBe(7.35);

    const saved = await saveModelPriceEntry({
      provider: "openai",
      modelPatterns: ["gpt-4o"],
      currency: "CNY",
      inputPerMillion: 19.0,
      cacheReadPerMillion: 9.5,
      cacheWritePerMillion: 19.0,
      outputPerMillion: 76.0,
      reasoningPerMillion: 0,
    });
    expect(saved).toBeDefined();
  });
});
