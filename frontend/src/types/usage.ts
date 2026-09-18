export interface CurrencyCost {
  currency: string;
  amount: number;
}

export interface UsageHeroOverview {
  totalTokens: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  reasoningTokens: number;
  requestsCount: number;
  costsByCurrency: CurrencyCost[];
  costCoveredTokens: number;
  costCoverageRatio: number;
  costCoveredRequests: number;
}

export interface UsageDailyTrendBucket {
  date: string;
  totalTokens: number;
  requestsCount: number;
  inputTokens: number;
  outputTokens: number;
  cacheTokens: number;
  costsByCurrency: CurrencyCost[];
}

export interface UsageModelBreakdown {
  model: string;
  provider: string;
  requestsCount: number;
  totalTokens: number;
  inputTokens: number;
  outputTokens: number;
  reasoningTokens: number;
  cacheTokens: number;
  cost: number | null;
  currency: string;
  costBasis: string;
}

export interface UsageSourceBreakdown {
  adapterId: string;
  sourceId: string;
  sourceName: string;
  appName: string;
  requestsCount: number;
  totalTokens: number;
  inputTokens: number;
  outputTokens: number;
  cacheTokens: number;
  costsByCurrency: CurrencyCost[];
  status: string;
  diagnostics: string[];
}

export interface UsageDateBreakdown {
  date: string;
  requestsCount: number;
  totalTokens: number;
  inputTokens: number;
  outputTokens: number;
  cacheTokens: number;
  costsByCurrency: CurrencyCost[];
}

export interface UsageSourceDiagnostic {
  sourceId: string;
  adapterId: string;
  level: string;
  message: string;
  timestamp: string;
}

export interface UsageScanStatus {
  scannedSourcesCount: number;
  totalEventsCount: number;
  lastScannedAt: string | null;
  activeScanTaskId: string | null;
  sourceDiagnostics: UsageSourceDiagnostic[];
}

export interface UsageDashboardDto {
  hero: UsageHeroOverview;
  dailyTrend: UsageDailyTrendBucket[];
  models: UsageModelBreakdown[];
  sources: UsageSourceBreakdown[];
  dates: UsageDateBreakdown[];
  scanStatus: UsageScanStatus;
}

export interface UsageDashboardFilter {
  timeRange?: string | null;
  adapterId?: string | null;
  sourceId?: string | null;
  model?: string | null;
  timezoneOffsetMinutes?: number | null;
}

export interface UsageScanOptions {
  mode?: "incremental" | "full" | null;
  sourceId?: string | null;
  adapterId?: string | null;
}

export function formatChineseTokens(tokens: number, precision = 2): string {
  if (!tokens || isNaN(tokens) || tokens === 0) {
    return "0";
  }
  const abs = Math.abs(tokens);
  const sign = tokens < 0 ? "-" : "";
  if (abs >= 100_000_000) {
    const val = (abs / 100_000_000).toFixed(precision);
    return `${sign}${val} 亿`;
  }
  if (abs >= 10_000) {
    const val = (abs / 10_000).toFixed(precision > 1 ? 1 : precision);
    return `${sign}${val} 万`;
  }
  return `${sign}${abs.toLocaleString()}`;
}

export function formatUsageCosts(costs: CurrencyCost[]): string {
  if (!costs || costs.length === 0) return "$0.00";
  return costs
    .map((c) => {
      const sym = c.currency.toUpperCase() === "CNY" ? "¥" : "$";
      return `${sym}${c.amount.toFixed(4).replace(/0+$/, "").replace(/\.$/, "")}`;
    })
    .join(" / ");
}

// Backward-compatibility aliases
export type ConversationUsageDashboard = UsageDashboardDto;
export type ConversationUsageScanStatus = UsageScanStatus;
export type ConversationUsageQuery = UsageDashboardFilter;
export type UsageTotals = UsageHeroOverview;
export type ModelUsageSummary = UsageModelBreakdown;
export type SourceUsageSummary = UsageSourceBreakdown;
export type DailyUsageTrendPoint = UsageDailyTrendBucket;
