// @vitest-environment jsdom

import {
  fireEvent,
  render,
  screen,
  cleanup,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ConversationPricingSettingsPanel } from "./ConversationPricingSettingsPanel";
import * as usageService from "@/services/usage";

describe("ConversationPricingSettingsPanel", () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  const mockStatus = {
    modelsCount: 3,
    localModelsCount: 5,
    exchangeRate: 7.25,
    activeChannel: "litellm",
    catalogVersion: "v1",
    lastUpdatedAt: new Date().toISOString(),
  };

  const mockCatalog = {
    catalogVersion: "bundled",
    updatedAt: new Date().toISOString(),
    usdToCnyRate: 7.25,
    activeChannel: "litellm",
    localModels: [
      {
        provider: "custom",
        modelPatterns: ["custom-unpriced-x"],
        rawModel: "custom-unpriced-x",
        matchedModelId: null,
        matchSource: "unmatched",
        priceLocked: false,
        isIgnored: false,
        isManualRebind: false,
        currency: "CNY",
        inputPerMillion: 0,
        cacheReadPerMillion: 0,
        cacheWritePerMillion: 0,
        outputPerMillion: 0,
        reasoningPerMillion: 0,
      },
      {
        provider: "openai",
        modelPatterns: ["gpt-4o"],
        rawModel: "gpt-4o",
        matchedModelId: "gpt-4o",
        matchSource: "auto_matched",
        priceLocked: false,
        isIgnored: false,
        isManualRebind: false,
        currency: "CNY",
        inputPerMillion: 18.13,
        cacheReadPerMillion: 9.06,
        cacheWritePerMillion: 18.13,
        outputPerMillion: 72.5,
        reasoningPerMillion: 0,
        rawCurrency: "USD",
        rawInputPerMillion: 2.5,
        rawOutputPerMillion: 10.0,
      },
      {
        provider: "anthropic",
        modelPatterns: ["Claude 3.7 Sonnet (High)"],
        rawModel: "Claude 3.7 Sonnet (High)",
        matchedModelId: "claude-3-7-sonnet*",
        matchSource: "manual_rebind",
        priceLocked: true,
        isIgnored: false,
        isManualRebind: true,
        currency: "CNY",
        inputPerMillion: 21.75,
        cacheReadPerMillion: 2.18,
        cacheWritePerMillion: 27.19,
        outputPerMillion: 108.75,
        reasoningPerMillion: 108.75,
        rawCurrency: "USD",
        rawInputPerMillion: 3.0,
        rawOutputPerMillion: 15.0,
      },
      {
        provider: "qwen",
        modelPatterns: ["qwen-2.5-free"],
        rawModel: "qwen-2.5-free",
        matchedModelId: "qwen-2.5-free",
        matchSource: "user_override",
        priceLocked: true,
        isIgnored: false,
        isManualRebind: false,
        currency: "CNY",
        inputPerMillion: 0,
        cacheReadPerMillion: 0,
        cacheWritePerMillion: 0,
        outputPerMillion: 0,
        reasoningPerMillion: 0,
      },
      {
        provider: "unknown",
        modelPatterns: ['ERROR: {"broken":true}'],
        rawModel: 'ERROR: {"broken":true}',
        matchedModelId: null,
        matchSource: "ignored",
        priceLocked: true,
        isIgnored: true,
        isManualRebind: false,
        currency: "CNY",
        inputPerMillion: 0,
        cacheReadPerMillion: 0,
        cacheWritePerMillion: 0,
        outputPerMillion: 0,
        reasoningPerMillion: 0,
      },
    ],
    models: [
      {
        provider: "openai",
        modelPatterns: ["gpt-4o"],
        rawModel: "gpt-4o",
        matchedModelId: "gpt-4o",
        matchSource: "reference",
        currency: "CNY",
        inputPerMillion: 18.13,
        cacheReadPerMillion: 9.06,
        cacheWritePerMillion: 18.13,
        outputPerMillion: 72.5,
        reasoningPerMillion: 0,
        rawCurrency: "USD",
        rawInputPerMillion: 2.5,
        rawOutputPerMillion: 10.0,
      },
      {
        provider: "anthropic",
        modelPatterns: ["claude-3-7-sonnet*"],
        rawModel: "claude-3-7-sonnet*",
        matchedModelId: "claude-3-7-sonnet*",
        matchSource: "reference",
        currency: "CNY",
        inputPerMillion: 21.75,
        cacheReadPerMillion: 2.18,
        cacheWritePerMillion: 27.19,
        outputPerMillion: 108.75,
        reasoningPerMillion: 108.75,
        rawCurrency: "USD",
        rawInputPerMillion: 3.0,
        rawOutputPerMillion: 15.0,
      },
      {
        provider: "deepseek",
        modelPatterns: ["deepseek-chat"],
        rawModel: "deepseek-chat",
        matchedModelId: "deepseek-chat",
        matchSource: "reference",
        currency: "CNY",
        inputPerMillion: 2.0,
        cacheReadPerMillion: 0.5,
        cacheWritePerMillion: 2.0,
        outputPerMillion: 8.0,
        reasoningPerMillion: 0,
      },
    ],
  };

  it("renders exchange rate and local models view by default with dual columns and consistent counts", async () => {
    vi.spyOn(usageService, "getModelPricingStatus").mockResolvedValue(
      mockStatus,
    );
    vi.spyOn(usageService, "getModelPricingCatalog").mockResolvedValue(
      mockCatalog,
    );

    render(<ConversationPricingSettingsPanel />);

    expect(await screen.findByText("参考汇率与在线同步")).toBeDefined();
    expect(screen.getByText("本地已用: 5 款")).toBeDefined();
    expect(screen.getByText("全网库: 3 款")).toBeDefined();
    expect(screen.getByText("渠道: litellm")).toBeDefined();

    // Two distinct columns
    expect(screen.getByText("原始模型名")).toBeDefined();
    expect(screen.getByText("匹配模型")).toBeDefined();

    // Status & lock badges
    expect(screen.getByText("未定价")).toBeDefined();
    expect(screen.getAllByText("已定价（免费）").length).toBe(2);
    expect(screen.getAllByText("已忽略").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("🔒 已锁定").length).toBeGreaterThanOrEqual(2);
    expect(screen.getAllByText("🔓 未锁定").length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText("Claude 3.7 Sonnet (High)")).toBeDefined();
    expect(screen.getByText("claude-3-7-sonnet*")).toBeDefined();
  });

  it("handles modifying exchange rate and saving", async () => {
    vi.spyOn(usageService, "getModelPricingStatus").mockResolvedValue(
      mockStatus,
    );
    vi.spyOn(usageService, "getModelPricingCatalog").mockResolvedValue(
      mockCatalog,
    );
    const setRateSpy = vi
      .spyOn(usageService, "setModelPricingExchangeRate")
      .mockResolvedValue({
        ...mockStatus,
        exchangeRate: 7.35,
      });

    render(<ConversationPricingSettingsPanel />);

    await screen.findByText("参考汇率与在线同步");

    const input = screen.getByDisplayValue("7.25");
    fireEvent.change(input, { target: { value: "7.35" } });

    const saveBtn = screen.getByRole("button", { name: /保存汇率并重算/ });
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(setRateSpy).toHaveBeenCalledWith(7.35);
    });
  });

  it("triggers online pricing synchronization", async () => {
    vi.spyOn(usageService, "getModelPricingStatus").mockResolvedValue(
      mockStatus,
    );
    vi.spyOn(usageService, "getModelPricingCatalog").mockResolvedValue(
      mockCatalog,
    );
    const syncSpy = vi
      .spyOn(usageService, "updateModelPricingCatalog")
      .mockResolvedValue({
        id: "task-sync-1",
        status: "completed",
        force: true,
        progress: {
          phase: "completed",
          modelsCount: 42,
          recalculatedEventsCount: 15,
          exchangeRate: 7.25,
        },
        startedAt: new Date().toISOString(),
        result: {
          channelUsed: "litellm",
          modelsCount: 42,
          exchangeRate: 7.25,
          recalculatedEventsCount: 15,
          catalogVersion: "v1",
          updatedAt: new Date().toISOString(),
        },
      });

    render(<ConversationPricingSettingsPanel />);

    await screen.findByText("参考汇率与在线同步");

    const syncBtn = screen.getByRole("button", { name: /在线同步最新价格/ });
    fireEvent.click(syncBtn);

    await waitFor(() => {
      expect(syncSpy).toHaveBeenCalledWith(true);
    });
  });

  it("searches and filters models", async () => {
    vi.spyOn(usageService, "getModelPricingStatus").mockResolvedValue(
      mockStatus,
    );
    vi.spyOn(usageService, "getModelPricingCatalog").mockResolvedValue(
      mockCatalog,
    );

    render(<ConversationPricingSettingsPanel />);

    await screen.findByText("Claude 3.7 Sonnet (High)");

    const searchInput = screen.getByPlaceholderText("搜索模型或厂商...");
    fireEvent.change(searchInput, { target: { value: "claude" } });

    expect(screen.queryByText("custom-unpriced-x")).toBeNull();
    expect(screen.getByText("Claude 3.7 Sonnet (High)")).toBeDefined();
  });

  it("supports editing a model price and saving it", async () => {
    vi.spyOn(usageService, "getModelPricingStatus").mockResolvedValue(
      mockStatus,
    );
    vi.spyOn(usageService, "getModelPricingCatalog").mockResolvedValue(
      mockCatalog,
    );
    const saveEntrySpy = vi
      .spyOn(usageService, "saveModelPriceEntry")
      .mockResolvedValue(mockStatus);

    render(<ConversationPricingSettingsPanel />);

    await screen.findByText("custom-unpriced-x");

    const editBtns = screen.getAllByRole("button", { name: /编辑/ });
    fireEvent.click(editBtns[0]);

    const saveModelBtn = screen.getByTitle("保存");
    expect(saveModelBtn).toBeDefined();

    fireEvent.click(saveModelBtn);

    await waitFor(() => {
      expect(saveEntrySpy).toHaveBeenCalledTimes(1);
    });
  });

  it("supports searchable base-model picker rebinding and unbinding", async () => {
    vi.spyOn(usageService, "getModelPricingStatus").mockResolvedValue(
      mockStatus,
    );
    vi.spyOn(usageService, "getModelPricingCatalog").mockResolvedValue(
      mockCatalog,
    );
    const rebindSpy = vi
      .spyOn(usageService, "rebindModelPricingAlias")
      .mockResolvedValue(mockStatus);

    render(<ConversationPricingSettingsPanel />);
    await screen.findByText("custom-unpriced-x");

    // Open picker for first row (custom-unpriced-x)
    const rebindBtns = screen.getAllByRole("button", { name: /改绑/ });
    fireEvent.click(rebindBtns[0]);

    const pickerSearch =
      await screen.findByPlaceholderText("搜索全网参考底座模型或厂商...");
    fireEvent.change(pickerSearch, { target: { value: "deepseek" } });

    const deepseekOption = await screen.findByRole("button", {
      name: /deepseek-chat/,
    });
    fireEvent.click(deepseekOption);

    await waitFor(() => {
      expect(rebindSpy).toHaveBeenCalledWith(
        "custom-unpriced-x",
        "deepseek-chat",
      );
    });

    // Unbind manual rebind row (Claude 3.7 Sonnet (High))
    const unbindBtn = screen.getByRole("button", { name: /恢复自动/ });
    fireEvent.click(unbindBtn);

    await waitFor(() => {
      expect(rebindSpy).toHaveBeenCalledWith("Claude 3.7 Sonnet (High)", null);
    });
  });

  it("supports unlocking locked prices and ignoring/restoring raw models", async () => {
    vi.spyOn(usageService, "getModelPricingStatus").mockResolvedValue(
      mockStatus,
    );
    vi.spyOn(usageService, "getModelPricingCatalog").mockResolvedValue(
      mockCatalog,
    );
    const unlockSpy = vi
      .spyOn(usageService, "setModelPricingLock")
      .mockResolvedValue(mockStatus);
    const ignoreSpy = vi
      .spyOn(usageService, "setModelPricingIgnored")
      .mockResolvedValue(mockStatus);

    render(<ConversationPricingSettingsPanel />);
    await screen.findByText("custom-unpriced-x");

    // Unlock locked price row
    const unlockBtns = screen.getAllByRole("button", { name: /解锁/ });
    fireEvent.click(unlockBtns[0]);

    await waitFor(() => {
      expect(unlockSpy).toHaveBeenCalledWith("claude-3-7-sonnet*", false);
    });

    // Ignore unmatched row
    const ignoreBtn = screen.getByRole("button", { name: /^忽略$/ });
    fireEvent.click(ignoreBtn);

    await waitFor(() => {
      expect(ignoreSpy).toHaveBeenCalledWith(
        "custom-unpriced-x",
        "custom",
        true,
      );
    });

    // Restore ignored row
    const restoreBtn = screen.getByRole("button", { name: /^恢复$/ });
    fireEvent.click(restoreBtn);

    await waitFor(() => {
      expect(ignoreSpy).toHaveBeenCalledWith(
        'ERROR: {"broken":true}',
        "unknown",
        false,
      );
    });
  });

  it("keeps the reference catalog tab strictly read-only without edit/lock/ignore controls", async () => {
    vi.spyOn(usageService, "getModelPricingStatus").mockResolvedValue(
      mockStatus,
    );
    vi.spyOn(usageService, "getModelPricingCatalog").mockResolvedValue(
      mockCatalog,
    );

    render(<ConversationPricingSettingsPanel />);
    await screen.findByText("custom-unpriced-x");

    const allTabBtn = screen.getByRole("button", { name: /全网参考库/ });
    fireEvent.click(allTabBtn);

    expect(await screen.findByText("deepseek-chat")).toBeDefined();
    expect(screen.queryByRole("button", { name: /编辑/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /解锁/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /改绑/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /^忽略$/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /^恢复$/ })).toBeNull();
    expect(screen.queryByText("操作")).toBeNull();
  });

  it("preserves backend sorting order and paginates local models at 15 per page", async () => {
    const manyLocal = Array.from({ length: 18 }, (_, i) => ({
      provider: i === 0 ? "zzz" : "openai",
      modelPatterns: [i === 0 ? "unpriced-top" : `model-${i}`],
      rawModel: i === 0 ? "unpriced-top" : `model-${i}`,
      matchedModelId: i === 0 ? null : `model-${i}`,
      matchSource: i === 0 ? "unmatched" : "auto_matched",
      priceLocked: false,
      isIgnored: false,
      isManualRebind: false,
      currency: "CNY",
      inputPerMillion: i === 0 ? 0 : 10,
      cacheReadPerMillion: 0,
      cacheWritePerMillion: 0,
      outputPerMillion: i === 0 ? 0 : 20,
      reasoningPerMillion: 0,
    }));

    vi.spyOn(usageService, "getModelPricingStatus").mockResolvedValue({
      ...mockStatus,
      localModelsCount: 18,
    });
    vi.spyOn(usageService, "getModelPricingCatalog").mockResolvedValue({
      ...mockCatalog,
      localModels: manyLocal,
    });

    render(<ConversationPricingSettingsPanel />);

    // Row 0 (unpriced-top) appears on page 1, model-15 appears on page 2
    expect(await screen.findByText("unpriced-top")).toBeDefined();
    expect(screen.getByText("1 / 2")).toBeDefined();
    expect(screen.queryByText("model-15")).toBeNull();

    // Go to page 2
    const nextBtn = screen.getByRole("button", { name: "下一页" });
    fireEvent.click(nextBtn);

    expect(
      (await screen.findAllByText("model-15")).length,
    ).toBeGreaterThanOrEqual(1);
    expect(screen.queryByText("unpriced-top")).toBeNull();
  });
});
