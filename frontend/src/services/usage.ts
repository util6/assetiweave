import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { TaskView } from "../types/taskCenter";
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
      matchedModelId: "gpt-5.0-turbo",
      rawModels: ["gpt-5.0-turbo", "gpt-5.0-turbo-2026-08-01"],
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
      unitPricePerMillion: 18.13,
    },
    {
      model: "gemini-3.8-flash",
      matchedModelId: "gemini-3.8-flash",
      rawModels: [
        "Flash -N",
        "Gemini 3.8 Flash (High)",
        "gemini-3.8-flash-free",
      ],
      provider: "google",
      requestsCount: 3800,
      totalTokens: 561000000,
      inputTokens: 561000000,
      outputTokens: 2135000,
      reasoningTokens: 40000,
      cacheTokens: 535000000,
      cost: 95.4,
      currency: "USD",
      costBasis: "catalog_estimated",
      unitPricePerMillion: 0.54,
    },
    {
      model: "qwen-2.5-free",
      matchedModelId: "qwen-2.5-free",
      rawModels: ["qwen-2.5-free"],
      provider: "qwen",
      requestsCount: 688,
      totalTokens: 28000000,
      inputTokens: 28000000,
      outputTokens: 98000,
      reasoningTokens: 0,
      cacheTokens: 18000000,
      cost: 0,
      currency: "CNY",
      costBasis: "catalog_estimated",
      unitPricePerMillion: 0,
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
  projects: [
    {
      projectPath: "~/code-space/assetiweave",
      displayName: "assetiweave",
      isUnassigned: false,
      requestsCount: 22000,
      totalTokens: 3200000000,
      inputTokens: 3192000000,
      outputTokens: 8000000,
      cacheTokens: 3100000000,
      costsByCurrency: [{ currency: "USD", amount: 530.5 }],
    },
    {
      projectPath: "__unassigned__",
      displayName: "未分配项目",
      isUnassigned: true,
      requestsCount: 938,
      totalTokens: 130000000,
      inputTokens: 129000000,
      outputTokens: 1000000,
      cacheTokens: 118000000,
      costsByCurrency: [{ currency: "USD", amount: 25.19 }],
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

const MOCK_REFERENCE_MODELS: ModelPriceEntry[] = [
  {
    provider: "openai",
    modelPatterns: ["gpt-4o", "gpt-4o-*"],
    rawModel: "gpt-4o",
    matchedModelId: "gpt-4o",
    matchSource: "reference",
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
    provider: "openai",
    modelPatterns: ["gpt-5.0-turbo"],
    rawModel: "gpt-5.0-turbo",
    matchedModelId: "gpt-5.0-turbo",
    matchSource: "reference",
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
    modelPatterns: ["claude-3-7-sonnet*"],
    rawModel: "claude-3-7-sonnet*",
    matchedModelId: "claude-3-7-sonnet*",
    matchSource: "reference",
    priceLocked: false,
    isIgnored: false,
    isManualRebind: false,
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
    rawModel: "deepseek-chat",
    matchedModelId: "deepseek-chat",
    matchSource: "reference",
    priceLocked: false,
    isIgnored: false,
    isManualRebind: false,
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
  {
    provider: "google",
    modelPatterns: ["gemini-3.8-flash"],
    rawModel: "gemini-3.8-flash",
    matchedModelId: "gemini-3.8-flash",
    matchSource: "reference",
    priceLocked: false,
    isIgnored: false,
    isManualRebind: false,
    currency: "CNY",
    inputPerMillion: 0.54,
    cacheReadPerMillion: 0.14,
    cacheWritePerMillion: 0.54,
    outputPerMillion: 2.18,
    reasoningPerMillion: 0,
    rawCurrency: "USD",
    rawInputPerMillion: 0.075,
    rawOutputPerMillion: 0.3,
  },
  {
    provider: "qwen",
    modelPatterns: ["qwen-2.5-free"],
    rawModel: "qwen-2.5-free",
    matchedModelId: "qwen-2.5-free",
    matchSource: "reference",
    priceLocked: false,
    isIgnored: false,
    isManualRebind: false,
    currency: "CNY",
    inputPerMillion: 1.0,
    cacheReadPerMillion: 0.25,
    cacheWritePerMillion: 1.0,
    outputPerMillion: 4.0,
    reasoningPerMillion: 0,
    rawCurrency: "CNY",
    rawInputPerMillion: 1.0,
    rawOutputPerMillion: 4.0,
  },
];

interface MockLocalUsageSeed {
  rawModel: string;
  provider: string;
}

const MOCK_LOCAL_SEEDS: MockLocalUsageSeed[] = [
  { rawModel: "custom-new-model-x", provider: "custom" },
  { rawModel: "Gemini 3.8 Flash (High)", provider: "google" },
  { rawModel: "gpt-4o", provider: "openai" },
  { rawModel: "qwen-2.5-free", provider: "qwen" },
  { rawModel: 'ERROR: {"model":"broken"} line\nbreak', provider: "unknown" },
];

const mockPricingState: {
  exchangeRate: number;
  activeChannel: string;
  catalogVersion: string;
  lastUpdatedAt: string;
  manualAliases: Record<string, string>;
  lockedOverrides: Record<string, ModelPriceEntry>;
  ignoredRawModels: Record<string, boolean>;
} = {
  exchangeRate: 7.25,
  activeChannel: "litellm",
  catalogVersion: "litellm-2026-10-05",
  lastUpdatedAt: new Date().toISOString(),
  manualAliases: {},
  lockedOverrides: {
    "qwen-2.5-free": {
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
  },
  ignoredRawModels: {
    'ERROR: {"model":"broken"} line\nbreak': true,
  },
};

function findMockReferenceById(modelId: string): ModelPriceEntry | undefined {
  return MOCK_REFERENCE_MODELS.find(
    (m) =>
      m.matchedModelId === modelId ||
      m.rawModel === modelId ||
      m.modelPatterns[0] === modelId,
  );
}

function autoMatchMockReference(rawModel: string): ModelPriceEntry | undefined {
  if (/["{}\n\r]/.test(rawModel)) return undefined;
  const normalized = rawModel
    .replace(/\s*\((high|medium|low|minimal|thinking)\)\s*$/i, "")
    .trim()
    .toLowerCase()
    .replace(/\s+/g, "-")
    .replace(/(-n|-tail|-free)$/i, "");
  return MOCK_REFERENCE_MODELS.find((m) =>
    m.modelPatterns.some((pat) => {
      const p = pat.toLowerCase();
      if (p.endsWith("*")) {
        return normalized.startsWith(p.slice(0, -1));
      }
      return p === normalized;
    }),
  );
}

function sortMockLocalEntries(entries: ModelPriceEntry[]): ModelPriceEntry[] {
  return [...entries].sort((a, b) => {
    const aUnpriced = !a.isIgnored && !a.matchedModelId ? 0 : 1;
    const bUnpriced = !b.isIgnored && !b.matchedModelId ? 0 : 1;
    if (aUnpriced !== bUnpriced) return aUnpriced - bUnpriced;
    const pCmp = a.provider.localeCompare(b.provider);
    if (pCmp !== 0) return pCmp;
    const aName = a.rawModel || a.modelPatterns[0] || "";
    const bName = b.rawModel || b.modelPatterns[0] || "";
    return aName.localeCompare(bName);
  });
}

function buildMockLocalModels(): ModelPriceEntry[] {
  const entries: ModelPriceEntry[] = MOCK_LOCAL_SEEDS.map((seed) => {
    if (mockPricingState.ignoredRawModels[seed.rawModel]) {
      return {
        provider: seed.provider,
        modelPatterns: [seed.rawModel],
        rawModel: seed.rawModel,
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
      };
    }

    const manualTargetId = mockPricingState.manualAliases[seed.rawModel];
    if (manualTargetId) {
      const locked = mockPricingState.lockedOverrides[manualTargetId];
      const ref = findMockReferenceById(manualTargetId);
      const source = locked ?? ref;
      return {
        provider: source?.provider ?? seed.provider,
        modelPatterns: [seed.rawModel],
        rawModel: seed.rawModel,
        matchedModelId: manualTargetId,
        matchSource: "manual_rebind",
        priceLocked: Boolean(locked?.priceLocked),
        isIgnored: false,
        isManualRebind: true,
        currency: "CNY",
        inputPerMillion: source?.inputPerMillion ?? 0,
        cacheReadPerMillion: source?.cacheReadPerMillion ?? 0,
        cacheWritePerMillion: source?.cacheWritePerMillion ?? 0,
        outputPerMillion: source?.outputPerMillion ?? 0,
        reasoningPerMillion: source?.reasoningPerMillion ?? 0,
      };
    }

    const directLocked = mockPricingState.lockedOverrides[seed.rawModel];
    if (directLocked) {
      return {
        ...directLocked,
        rawModel: seed.rawModel,
        modelPatterns: [seed.rawModel],
        matchedModelId: directLocked.matchedModelId ?? seed.rawModel,
        matchSource: "user_override",
        priceLocked: true,
        isIgnored: false,
        isManualRebind: false,
      };
    }

    const autoRef = autoMatchMockReference(seed.rawModel);
    if (autoRef) {
      const baseId = autoRef.matchedModelId ?? autoRef.modelPatterns[0];
      const baseLocked = mockPricingState.lockedOverrides[baseId];
      const effective = baseLocked ?? autoRef;
      return {
        provider: effective.provider,
        modelPatterns: [seed.rawModel],
        rawModel: seed.rawModel,
        matchedModelId: baseId,
        matchSource: baseLocked ? "user_override" : "auto_matched",
        priceLocked: Boolean(baseLocked?.priceLocked),
        isIgnored: false,
        isManualRebind: false,
        currency: "CNY",
        inputPerMillion: effective.inputPerMillion,
        cacheReadPerMillion: effective.cacheReadPerMillion,
        cacheWritePerMillion: effective.cacheWritePerMillion,
        outputPerMillion: effective.outputPerMillion,
        reasoningPerMillion: effective.reasoningPerMillion,
      };
    }

    return {
      provider: seed.provider,
      modelPatterns: [seed.rawModel],
      rawModel: seed.rawModel,
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
    };
  });

  return sortMockLocalEntries(entries);
}

function buildMockPricingStatus(): ModelPricingStatus {
  const localModels = buildMockLocalModels();
  return {
    modelsCount: MOCK_REFERENCE_MODELS.length,
    localModelsCount: localModels.length,
    exchangeRate: mockPricingState.exchangeRate,
    activeChannel: mockPricingState.activeChannel,
    catalogVersion: mockPricingState.catalogVersion,
    lastUpdatedAt: mockPricingState.lastUpdatedAt,
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
        modelsCount: models ? models.length : MOCK_REFERENCE_MODELS.length,
        recalculatedEventsCount: 22938,
        exchangeRate: mockPricingState.exchangeRate,
      },
      startedAt: new Date().toISOString(),
      finishedAt: new Date().toISOString(),
      result: {
        channelUsed: "litellm",
        modelsCount: models ? models.length : MOCK_REFERENCE_MODELS.length,
        exchangeRate: mockPricingState.exchangeRate,
        recalculatedEventsCount: 22938,
        catalogVersion: mockPricingState.catalogVersion,
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
    return buildMockPricingStatus();
  }
  return invoke<ModelPricingStatus>("get_model_pricing_status");
}

export async function getModelPricingCatalog(): Promise<ModelPricingCatalog> {
  if (!isTauriRuntime()) {
    return {
      catalogVersion: mockPricingState.catalogVersion,
      updatedAt: mockPricingState.lastUpdatedAt,
      usdToCnyRate: mockPricingState.exchangeRate,
      activeChannel: mockPricingState.activeChannel,
      models: MOCK_REFERENCE_MODELS.map((m) => ({ ...m })),
      localModels: buildMockLocalModels(),
    };
  }
  return invoke<ModelPricingCatalog>("get_model_pricing_catalog");
}

export function subscribePricingSyncTask(
  listener: (task: TaskView) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) {
    return Promise.resolve(() => undefined);
  }
  return listen<TaskView>("task-updated", (event) => {
    if (event.payload.kind === "modelpricingsync") {
      listener(event.payload);
    }
  });
}

export async function setModelPricingExchangeRate(
  rate: number,
): Promise<ModelPricingStatus> {
  if (!isTauriRuntime()) {
    mockPricingState.exchangeRate = rate;
    mockPricingState.activeChannel = "custom";
    mockPricingState.catalogVersion = "custom";
    mockPricingState.lastUpdatedAt = new Date().toISOString();
    return buildMockPricingStatus();
  }
  return invoke<ModelPricingStatus>("set_model_pricing_exchange_rate", {
    rate,
  });
}

export async function saveModelPriceEntry(
  entry: ModelPriceEntry,
): Promise<ModelPricingStatus> {
  if (!isTauriRuntime()) {
    const baseId =
      entry.matchedModelId || entry.rawModel || entry.modelPatterns[0];
    if (baseId) {
      mockPricingState.lockedOverrides[baseId] = {
        ...entry,
        modelPatterns: [baseId],
        matchedModelId: baseId,
        matchSource: "user_override",
        priceLocked: true,
        isIgnored: false,
      };
    }
    mockPricingState.activeChannel = "custom";
    mockPricingState.catalogVersion = "custom";
    mockPricingState.lastUpdatedAt = new Date().toISOString();
    return buildMockPricingStatus();
  }
  return invoke<ModelPricingStatus>("save_model_price_entry", {
    entry,
  });
}

export async function rebindModelPricingAlias(
  rawModel: string,
  targetModelId: string | null,
): Promise<ModelPricingStatus> {
  if (!isTauriRuntime()) {
    if (targetModelId === null) {
      delete mockPricingState.manualAliases[rawModel];
    } else {
      delete mockPricingState.ignoredRawModels[rawModel];
      mockPricingState.manualAliases[rawModel] = targetModelId;
    }
    mockPricingState.lastUpdatedAt = new Date().toISOString();
    return buildMockPricingStatus();
  }
  return invoke<ModelPricingStatus>("rebind_model_pricing_alias", {
    rawModel,
    targetModelId,
  });
}

export async function setModelPricingLock(
  modelId: string,
  locked: boolean,
): Promise<ModelPricingStatus> {
  if (!isTauriRuntime()) {
    if (!locked) {
      delete mockPricingState.lockedOverrides[modelId];
    } else if (!mockPricingState.lockedOverrides[modelId]) {
      const ref = findMockReferenceById(modelId);
      if (ref) {
        mockPricingState.lockedOverrides[modelId] = {
          ...ref,
          matchedModelId: modelId,
          matchSource: "user_override",
          priceLocked: true,
        };
      }
    }
    mockPricingState.lastUpdatedAt = new Date().toISOString();
    return buildMockPricingStatus();
  }
  return invoke<ModelPricingStatus>("set_model_pricing_lock", {
    modelId,
    locked,
  });
}

export async function setModelPricingIgnored(
  rawModel: string,
  provider: string | null,
  ignored: boolean,
): Promise<ModelPricingStatus> {
  if (!isTauriRuntime()) {
    if (ignored) {
      delete mockPricingState.manualAliases[rawModel];
      mockPricingState.ignoredRawModels[rawModel] = true;
    } else {
      delete mockPricingState.ignoredRawModels[rawModel];
    }
    mockPricingState.lastUpdatedAt = new Date().toISOString();
    return buildMockPricingStatus();
  }
  return invoke<ModelPricingStatus>("set_model_pricing_ignored", {
    rawModel,
    provider,
    ignored,
  });
}
