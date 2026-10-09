// @vitest-environment jsdom

import { fireEvent, render, screen, cleanup, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ConversationPricingSettingsPanel } from "./ConversationPricingSettingsPanel";
import * as usageService from "@/services/usage";

describe("ConversationPricingSettingsPanel", () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  const mockStatus = {
    modelsCount: 42,
    localModelsCount: 2,
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
        provider: "openai",
        modelPatterns: ["gpt-4o"],
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
    ],
    models: [
      {
        provider: "openai",
        modelPatterns: ["gpt-4o"],
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
    ],
  };

  it("renders exchange rate and local models view by default", async () => {
    vi.spyOn(usageService, "getModelPricingStatus").mockResolvedValue(mockStatus);
    vi.spyOn(usageService, "getModelPricingCatalog").mockResolvedValue(mockCatalog);

    render(<ConversationPricingSettingsPanel />);

    expect(await screen.findByText("参考汇率与在线同步")).toBeDefined();
    expect(screen.getByText("本地已用: 2 款")).toBeDefined();
    expect(screen.getByText("全网库: 42 款")).toBeDefined();
    expect(screen.getByText("渠道: litellm")).toBeDefined();
    expect(screen.getByText("gpt-4o")).toBeDefined();
    expect(screen.getByText("claude-3-7-sonnet*")).toBeDefined();
  });

  it("handles modifying exchange rate and saving", async () => {
    vi.spyOn(usageService, "getModelPricingStatus").mockResolvedValue(mockStatus);
    vi.spyOn(usageService, "getModelPricingCatalog").mockResolvedValue(mockCatalog);
    const setRateSpy = vi.spyOn(usageService, "setModelPricingExchangeRate").mockResolvedValue({
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
    vi.spyOn(usageService, "getModelPricingStatus").mockResolvedValue(mockStatus);
    vi.spyOn(usageService, "getModelPricingCatalog").mockResolvedValue(mockCatalog);
    const syncSpy = vi.spyOn(usageService, "updateModelPricingCatalog").mockResolvedValue({
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
    vi.spyOn(usageService, "getModelPricingStatus").mockResolvedValue(mockStatus);
    vi.spyOn(usageService, "getModelPricingCatalog").mockResolvedValue(mockCatalog);

    render(<ConversationPricingSettingsPanel />);

    await screen.findByText("claude-3-7-sonnet*");

    const searchInput = screen.getByPlaceholderText("搜索模型或厂商...");
    fireEvent.change(searchInput, { target: { value: "claude" } });

    expect(screen.queryByText("gpt-4o")).toBeNull();
    expect(screen.getByText("claude-3-7-sonnet*")).toBeDefined();
  });

  it("supports editing a model price and saving it", async () => {
    vi.spyOn(usageService, "getModelPricingStatus").mockResolvedValue(mockStatus);
    vi.spyOn(usageService, "getModelPricingCatalog").mockResolvedValue(mockCatalog);
    const saveEntrySpy = vi.spyOn(usageService, "saveModelPriceEntry").mockResolvedValue(mockStatus);

    render(<ConversationPricingSettingsPanel />);

    await screen.findByText("gpt-4o");

    const editBtns = screen.getAllByRole("button", { name: /编辑/ });
    fireEvent.click(editBtns[0]);

    const saveModelBtn = screen.getByTitle("保存");
    expect(saveModelBtn).toBeDefined();

    fireEvent.click(saveModelBtn);

    await waitFor(() => {
      expect(saveEntrySpy).toHaveBeenCalledTimes(1);
    });
  });
});
