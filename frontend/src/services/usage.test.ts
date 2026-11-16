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
    expect(snapshot.state).toBe("succeeded");

    const selectiveSnapshot = await updateModelPricingCatalog(false, [
      "gpt-5.6-luna",
      "gemini-3.8-flash",
    ]);
    expect(selectiveSnapshot.id).toBeDefined();
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

  it("keeps status.modelsCount consistent with catalog.models.length and provides rich localModels & rawModels", async () => {
    const {
      getModelPricingStatus,
      getModelPricingCatalog,
      getConversationUsageDashboard,
    } = await import("./usage");

    const [status, catalog, dashboard] = await Promise.all([
      getModelPricingStatus(),
      getModelPricingCatalog(),
      getConversationUsageDashboard(),
    ]);

    expect(status.modelsCount).toBe(catalog.models.length);
    expect(catalog.localModels).toBeDefined();
    expect(catalog.localModels!.length).toBeGreaterThanOrEqual(4);
    expect(status.localModelsCount).toBe(catalog.localModels!.length);

    // Unmatched entry sorted at top
    const first = catalog.localModels![0];
    expect(first.matchSource).toBe("unmatched");
    expect(first.matchedModelId).toBeNull();

    // Includes locked ¥0 free, auto_matched variant, and ignored dirty string
    expect(
      catalog.localModels!.some(
        (m) =>
          m.priceLocked === true &&
          m.inputPerMillion === 0 &&
          m.outputPerMillion === 0 &&
          !m.isIgnored,
      ),
    ).toBe(true);
    expect(
      catalog.localModels!.some(
        (m) => m.isIgnored === true && m.matchSource === "ignored",
      ),
    ).toBe(true);
    expect(
      catalog.localModels!.some(
        (m) =>
          m.matchSource === "auto_matched" &&
          Boolean(m.rawModel) &&
          Boolean(m.matchedModelId) &&
          m.rawModel !== m.matchedModelId,
      ),
    ).toBe(true);

    // Dashboard models include rawModels arrays
    expect(
      dashboard.models.some(
        (m) => Array.isArray(m.rawModels) && m.rawModels.length > 1,
      ),
    ).toBe(true);
  });

  it("supports rebindModelPricingAlias, setModelPricingLock, and setModelPricingIgnored in mock mode", async () => {
    const {
      getModelPricingCatalog,
      rebindModelPricingAlias,
      saveModelPriceEntry,
      setModelPricingLock,
      setModelPricingIgnored,
    } = await import("./usage");

    const before = await getModelPricingCatalog();
    const unmatched = before.localModels!.find(
      (m) => m.matchSource === "unmatched",
    )!;
    expect(unmatched).toBeDefined();
    const rawName = unmatched.rawModel || unmatched.modelPatterns[0];

    // 1. Rebind to an existing reference model
    await rebindModelPricingAlias(rawName, "deepseek-chat");
    let after = await getModelPricingCatalog();
    let rebound = after.localModels!.find(
      (m) => (m.rawModel || m.modelPatterns[0]) === rawName,
    )!;
    expect(rebound.matchedModelId).toBe("deepseek-chat");
    expect(rebound.matchSource).toBe("manual_rebind");
    expect(rebound.isManualRebind).toBe(true);
    expect(rebound.inputPerMillion).toBe(2.0);

    // 2. Edit price -> auto-locks
    await saveModelPriceEntry({
      ...rebound,
      inputPerMillion: 3.5,
      outputPerMillion: 9.0,
    });
    after = await getModelPricingCatalog();
    rebound = after.localModels!.find(
      (m) => (m.rawModel || m.modelPatterns[0]) === rawName,
    )!;
    expect(rebound.priceLocked).toBe(true);
    expect(rebound.inputPerMillion).toBe(3.5);

    // 3. Unlock -> reverts to reference price
    await setModelPricingLock("deepseek-chat", false);
    after = await getModelPricingCatalog();
    rebound = after.localModels!.find(
      (m) => (m.rawModel || m.modelPatterns[0]) === rawName,
    )!;
    expect(rebound.priceLocked).toBe(false);
    expect(rebound.inputPerMillion).toBe(2.0);

    // 4. Unbind manual alias -> returns to unmatched
    await rebindModelPricingAlias(rawName, null);
    after = await getModelPricingCatalog();
    rebound = after.localModels!.find(
      (m) => (m.rawModel || m.modelPatterns[0]) === rawName,
    )!;
    expect(rebound.isManualRebind).toBe(false);
    expect(rebound.matchedModelId).toBeNull();
    expect(rebound.matchSource).toBe("unmatched");

    // 5. Ignore and restore
    await setModelPricingIgnored(rawName, unmatched.provider, true);
    after = await getModelPricingCatalog();
    rebound = after.localModels!.find(
      (m) => (m.rawModel || m.modelPatterns[0]) === rawName,
    )!;
    expect(rebound.isIgnored).toBe(true);
    expect(rebound.matchSource).toBe("ignored");

    await setModelPricingIgnored(rawName, unmatched.provider, false);
    after = await getModelPricingCatalog();
    rebound = after.localModels!.find(
      (m) => (m.rawModel || m.modelPatterns[0]) === rawName,
    )!;
    expect(rebound.isIgnored).toBe(false);
    expect(rebound.matchSource).toBe("unmatched");
  });
});
