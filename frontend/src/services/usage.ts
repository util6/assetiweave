import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  ModelPriceEntry,
  ModelPricingCatalog,
  ModelPricingStatus,
  ModelPricingSyncResult,
  ModelPricingSyncTaskSnapshot,
  UsageDashboardDto,
  UsageDashboardFilter,
  UsageScanOptions,
  UsageScanStatus,
} from "../types/usage";

export function isTauriRuntime(): boolean {
  return (
    typeof window !== "undefined" &&
    ("__TAURI_INTERNALS__" in window || "__TAURI__" in window)
  );
}

export const USAGE_SCAN_TASK_UPDATED_EVENT =
  "conversation-usage-scan-task-updated";

export interface UsageScanTaskSnapshot {
  id: string;
  status: "pending" | "running" | "completed" | "failed" | "cancelled" | string;
  sourceId?: string | null;
  mode: string;
  startedAt: string;
  finishedAt?: string | null;
}

export function subscribeUsageScanTask(
  listener: (snapshot: UsageScanTaskSnapshot) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) {
    return Promise.resolve(() => undefined);
  }
  return listen<UsageScanTaskSnapshot>(
    USAGE_SCAN_TASK_UPDATED_EVENT,
    (event) => {
      listener(event.payload);
    },
  );
}

const DEFAULT_MOCK_DASHBOARD: UsageDashboardDto = {
  hero: {
    totalTokens: 3330000000,
    totalInputTokens: 3322000000,
    totalOutputTokens: 7900000,
    cacheReadTokens: 3218000000,
    cacheWriteTokens: 0,
    reasoningTokens: 120000,
    requestsCount: 22938,
    costsByCurrency: [{ currency: "USD", amount: 555.6927 }],
    costCoveredTokens: 3330000000,
    costCoverageRatio: 1.0,
    costCoveredRequests: 22938,
  },
  dailyTrend: [
    {
      date: "2026-08-16",
      totalTokens: 85503000,
      requestsCount: 545,
      inputTokens: 85111000,
      outputTokens: 392000,
      cacheTokens: 82404000,
      costsByCurrency: [{ currency: "USD", amount: 14.2 }],
    },
    {
      date: "2026-08-20",
      totalTokens: 328000000,
      requestsCount: 2271,
      inputTokens: 325000000,
      outputTokens: 639000,
      cacheTokens: 317000000,
      costsByCurrency: [{ currency: "USD", amount: 54.8 }],
    },
    {
      date: "2026-08-25",
      totalTokens: 358000000,
      requestsCount: 2352,
      inputTokens: 356000000,
      outputTokens: 658000,
      cacheTokens: 346000000,
      costsByCurrency: [{ currency: "USD", amount: 59.2 }],
    },
    {
      date: "2026-09-01",
      totalTokens: 348000000,
      requestsCount: 2235,
      inputTokens: 345000000,
      outputTokens: 787000,
      cacheTokens: 337000000,
      costsByCurrency: [{ currency: "USD", amount: 58.1 }],
    },
    {
      date: "2026-09-08",
      totalTokens: 7257000,
      requestsCount: 48,
      inputTokens: 7220000,
      outputTokens: 37000,
      cacheTokens: 6849000,
      costsByCurrency: [{ currency: "USD", amount: 1.2 }],
    },
    {
      date: "2026-09-11",
      totalTokens: 18735000,
      requestsCount: 148,
      inputTokens: 18674000,
      outputTokens: 61000,
      cacheTokens: 17895000,
      costsByCurrency: [{ currency: "USD", amount: 3.1 }],
    },
    {
      date: "2026-09-14",
      totalTokens: 3788000,
      requestsCount: 52,
      inputTokens: 3751000,
      outputTokens: 38000,
      cacheTokens: 2832000,
      costsByCurrency: [{ currency: "USD", amount: 0.65 }],
    },
  ],
  models: [
    {
      model: "gpt-5.0-turbo",
      provider: "openai",
      requestsCount: 18450,
      totalTokens: 2741000000,
      inputTokens: 2741000000,
      outputTokens: 5667000,
      reasoningTokens: 80000,
      cacheTokens: 2665000000,
      cost: 450.2,
      currency: "USD",
      costBasis: "official",
    },
    {
      model: "gpt-4.5-preview",
      provider: "openai",
      requestsCount: 3800,
      totalTokens: 561000000,
      inputTokens: 561000000,
      outputTokens: 2135000,
      reasoningTokens: 40000,
      cacheTokens: 535000000,
      cost: 95.4,
      currency: "USD",
      costBasis: "official",
    },
    {
      model: "gpt-4-turbo",
      provider: "openai",
      requestsCount: 688,
      totalTokens: 28000000,
      inputTokens: 28000000,
      outputTokens: 98000,
      reasoningTokens: 0,
      cacheTokens: 18000000,
      cost: 10.09,
      currency: "USD",
      costBasis: "official",
    },
  ],
  sources: [
    {
      adapterId: "codex",
      sourceId: "src_codex_default",
      sourceName: "默认实例",
      appName: "Codex",
      requestsCount: 21500,
      totalTokens: 3101000000,
      inputTokens: 3101000000,
      outputTokens: 7780000,
      cacheTokens: 3100000000,
      costsByCurrency: [{ currency: "USD", amount: 512.4 }],
      status: "active",
      diagnostics: [],
    },
    {
      adapterId: "antigravity",
      sourceId: "src_antigravity_default",
      sourceName: "分支",
      appName: "Antigravity",
      requestsCount: 1438,
      totalTokens: 229000000,
      inputTokens: 229000000,
      outputTokens: 120000,
      cacheTokens: 118000000,
      costsByCurrency: [{ currency: "USD", amount: 43.29 }],
      status: "active",
      diagnostics: [],
    },
  ],
  dates: [
    {
      date: "2026-09-14",
      requestsCount: 52,
      totalTokens: 3788000,
      inputTokens: 3751000,
      outputTokens: 38000,
      cacheTokens: 2832000,
      costsByCurrency: [{ currency: "USD", amount: 0.65 }],
    },
    {
      date: "2026-09-11",
      requestsCount: 148,
      totalTokens: 18735000,
      inputTokens: 18674000,
      outputTokens: 61000,
      cacheTokens: 17895000,
      costsByCurrency: [{ currency: "USD", amount: 3.1 }],
    },
    {
      date: "2026-09-10",
      requestsCount: 201,
      totalTokens: 20973000,
      inputTokens: 20788000,
      outputTokens: 187000,
      cacheTokens: 18938000,
      costsByCurrency: [{ currency: "USD", amount: 3.5 }],
    },
    {
      date: "2026-09-09",
      requestsCount: 68,
      totalTokens: 6041000,
      inputTokens: 5991000,
      outputTokens: 50000,
      cacheTokens: 5218000,
      costsByCurrency: [{ currency: "USD", amount: 1.0 }],
    },
    {
      date: "2026-09-08",
      requestsCount: 434,
      totalTokens: 84820000,
      inputTokens: 84471000,
      outputTokens: 348000,
      cacheTokens: 81480000,
      costsByCurrency: [{ currency: "USD", amount: 14.1 }],
    },
    {
      date: "2026-09-07",
      requestsCount: 107,
      totalTokens: 9289000,
      inputTokens: 9227000,
      outputTokens: 62000,
      cacheTokens: 8352000,
      costsByCurrency: [{ currency: "USD", amount: 1.5 }],
    },
    {
      date: "2026-09-06",
      requestsCount: 68,
      totalTokens: 7257000,
      inputTokens: 7220000,
      outputTokens: 37000,
      cacheTokens: 6849000,
      costsByCurrency: [{ currency: "USD", amount: 1.2 }],
    },
    {
      date: "2026-09-05",
      requestsCount: 21,
      totalTokens: 1438000,
      inputTokens: 1422000,
      outputTokens: 14000,
      cacheTokens: 1330000,
      costsByCurrency: [{ currency: "USD", amount: 0.24 }],
    },
    {
      date: "2026-09-04",
      requestsCount: 148,
      totalTokens: 18838000,
      inputTokens: 18742000,
      outputTokens: 94000,
      cacheTokens: 17317000,
      costsByCurrency: [{ currency: "USD", amount: 3.1 }],
    },
    {
      date: "2026-09-03",
      requestsCount: 256,
      totalTokens: 32168000,
      inputTokens: 32022000,
      outputTokens: 144000,
      cacheTokens: 30313000,
      costsByCurrency: [{ currency: "USD", amount: 5.3 }],
    },
    {
      date: "2026-08-25",
      requestsCount: 2352,
      totalTokens: 358000000,
      inputTokens: 356000000,
      outputTokens: 658000,
      cacheTokens: 346000000,
      costsByCurrency: [{ currency: "USD", amount: 59.2 }],
    },
  ],
  scanStatus: {
    scannedSourcesCount: 464,
    totalEventsCount: 22938,
    lastScannedAt: new Date().toISOString(),
    activeScanTaskId: null,
    sourceDiagnostics: [],
  },
};

export async function getConversationUsageDashboard(
  filter: UsageDashboardFilter = {},
): Promise<UsageDashboardDto> {
  if (!isTauriRuntime()) {
    if (filter.startDate || filter.endDate) {
      const start = filter.startDate || "1970-01-01";
      const end = filter.endDate || "2099-12-31";
      const filteredTrend = DEFAULT_MOCK_DASHBOARD.dailyTrend.filter(
        (d) => d.date >= start && d.date <= end,
      );
      const filteredDates = DEFAULT_MOCK_DASHBOARD.dates.filter(
        (d) => d.date >= start && d.date <= end,
      );
      const totalTokens = filteredTrend.reduce(
        (sum, d) => sum + d.totalTokens,
        0,
      );
      const requestsCount = filteredTrend.reduce(
        (sum, d) => sum + d.requestsCount,
        0,
      );
      return {
        ...DEFAULT_MOCK_DASHBOARD,
        hero: {
          ...DEFAULT_MOCK_DASHBOARD.hero,
          totalTokens: filteredTrend.length > 0 ? totalTokens : 0,
          requestsCount: filteredTrend.length > 0 ? requestsCount : 0,
        },
        dailyTrend: filteredTrend,
        dates: filteredDates,
      };
    }
    return DEFAULT_MOCK_DASHBOARD;
  }
  return invoke<UsageDashboardDto>("get_conversation_usage_dashboard", {
    filter: filter ?? {},
  });
}

export async function getConversationUsageScanStatus(): Promise<UsageScanStatus> {
  if (!isTauriRuntime()) {
    return DEFAULT_MOCK_DASHBOARD.scanStatus;
  }
  return invoke<UsageScanStatus>("get_conversation_usage_scan_status");
}

export async function scanConversationUsage(
  options: UsageScanOptions = {},
): Promise<UsageScanStatus> {
  if (!isTauriRuntime()) {
    return {
      scannedSourcesCount: 464,
      totalEventsCount: 22938,
      lastScannedAt: new Date().toISOString(),
      activeScanTaskId: "mock-task-usage-scan",
      sourceDiagnostics: [],
    };
  }
  const snapshot = await invoke<{ id: string; status: string }>(
    "scan_conversation_usage",
    {
      options: options ?? {},
    },
  );
  const status = await getConversationUsageScanStatus();
  return {
    ...status,
    activeScanTaskId: snapshot?.id || status.activeScanTaskId,
  };
}

export async function updateModelPricingCatalog(
  force = false,
  models?: string[],
): Promise<ModelPricingSyncTaskSnapshot> {
  if (!isTauriRuntime()) {
    return {
      id: "mock-task-pricing-sync",
      status: "completed",
      force,
      progress: {
        phase: "completed",
        message: "价格同步完成",
        channel: "litellm",
        modelsCount: models ? models.length : 12,
        recalculatedEventsCount: 22938,
        exchangeRate: 7.25,
      },
      startedAt: new Date().toISOString(),
      finishedAt: new Date().toISOString(),
      result: {
        channelUsed: "litellm",
        modelsCount: models ? models.length : 12,
        exchangeRate: 7.25,
        recalculatedEventsCount: 22938,
        catalogVersion: "litellm-2026-10-05",
        updatedAt: new Date().toISOString(),
      },
    };
  }
  return invoke<ModelPricingSyncTaskSnapshot>("update_model_pricing_catalog", {
    force,
    models,
  });
}

export async function getModelPricingStatus(): Promise<ModelPricingStatus> {
  if (!isTauriRuntime()) {
    return {
      modelsCount: 1248,
      exchangeRate: 7.25,
      activeChannel: "litellm",
      catalogVersion: "litellm-2026-10-05",
      lastUpdatedAt: new Date().toISOString(),
    };
  }
  return invoke<ModelPricingStatus>("get_model_pricing_status");
}

export async function getModelPricingCatalog(): Promise<ModelPricingCatalog> {
  if (!isTauriRuntime()) {
    return {
      catalogVersion: "bundled",
      updatedAt: new Date().toISOString(),
      usdToCnyRate: 7.25,
      activeChannel: "litellm",
      models: [
        {
          provider: "openai",
          modelPatterns: ["gpt-4o", "gpt-4o-*"],
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
        {
          provider: "deepseek",
          modelPatterns: ["deepseek-chat", "deepseek-v3"],
          currency: "CNY",
          inputPerMillion: 2.0,
          cacheReadPerMillion: 0.5,
          cacheWritePerMillion: 2.0,
          outputPerMillion: 8.0,
          reasoningPerMillion: 0,
          rawCurrency: "CNY",
          rawInputPerMillion: 2.0,
          rawOutputPerMillion: 8.0,
        },
      ],
    };
  }
  return invoke<ModelPricingCatalog>("get_model_pricing_catalog");
}

export function subscribeModelPricingSyncTask(
  listener: (snapshot: ModelPricingSyncTaskSnapshot) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) {
    return Promise.resolve(() => undefined);
  }
  return listen<ModelPricingSyncTaskSnapshot>(
    "model-pricing-sync-task-updated",
    (event) => {
      listener(event.payload);
    },
  );
}

export async function setModelPricingExchangeRate(
  rate: number,
): Promise<ModelPricingStatus> {
  if (!isTauriRuntime()) {
    return {
      modelsCount: 1248,
      exchangeRate: rate,
      activeChannel: "custom",
      catalogVersion: "custom",
      lastUpdatedAt: new Date().toISOString(),
    };
  }
  return invoke<ModelPricingStatus>("set_model_pricing_exchange_rate", {
    rate,
  });
}

export async function saveModelPriceEntry(
  entry: ModelPriceEntry,
): Promise<ModelPricingStatus> {
  if (!isTauriRuntime()) {
    return {
      modelsCount: 1248,
      exchangeRate: 7.25,
      activeChannel: "custom",
      catalogVersion: "custom",
      lastUpdatedAt: new Date().toISOString(),
    };
  }
  return invoke<ModelPricingStatus>("save_model_price_entry", {
    entry,
  });
}
