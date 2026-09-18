use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CurrencyCostDto {
    pub(crate) currency: String,
    pub(crate) amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageDailyTrendBucketDto {
    pub(crate) date: String,
    pub(crate) total_tokens: i64,
    pub(crate) requests_count: i64,
    pub(crate) input_tokens: i64,
    pub(crate) output_tokens: i64,
    pub(crate) cache_tokens: i64,
    pub(crate) costs_by_currency: Vec<CurrencyCostDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageModelBreakdownDto {
    pub(crate) model: String,
    pub(crate) provider: String,
    pub(crate) requests_count: i64,
    pub(crate) total_tokens: i64,
    pub(crate) input_tokens: i64,
    pub(crate) output_tokens: i64,
    pub(crate) reasoning_tokens: i64,
    pub(crate) cache_tokens: i64,
    pub(crate) cost: Option<f64>,
    pub(crate) currency: String,
    pub(crate) cost_basis: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageSourceBreakdownDto {
    pub(crate) adapter_id: String,
    pub(crate) source_id: String,
    pub(crate) source_name: String,
    pub(crate) app_name: String,
    pub(crate) requests_count: i64,
    pub(crate) total_tokens: i64,
    pub(crate) input_tokens: i64,
    pub(crate) output_tokens: i64,
    pub(crate) cache_tokens: i64,
    pub(crate) costs_by_currency: Vec<CurrencyCostDto>,
    pub(crate) status: String,
    pub(crate) diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageDateBreakdownDto {
    pub(crate) date: String,
    pub(crate) requests_count: i64,
    pub(crate) total_tokens: i64,
    pub(crate) input_tokens: i64,
    pub(crate) output_tokens: i64,
    pub(crate) cache_tokens: i64,
    pub(crate) costs_by_currency: Vec<CurrencyCostDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageSourceDiagnosticDto {
    pub(crate) source_id: String,
    pub(crate) adapter_id: String,
    pub(crate) level: String,
    pub(crate) message: String,
    pub(crate) timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageHeroOverviewDto {
    pub(crate) total_tokens: i64,
    pub(crate) total_input_tokens: i64,
    pub(crate) total_output_tokens: i64,
    pub(crate) cache_read_tokens: i64,
    pub(crate) cache_write_tokens: i64,
    pub(crate) reasoning_tokens: i64,
    pub(crate) requests_count: i64,
    pub(crate) costs_by_currency: Vec<CurrencyCostDto>,
    pub(crate) cost_covered_tokens: i64,
    pub(crate) cost_coverage_ratio: f64,
    pub(crate) cost_covered_requests: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageScanStatusDto {
    pub(crate) scanned_sources_count: usize,
    pub(crate) total_events_count: usize,
    pub(crate) last_scanned_at: Option<String>,
    pub(crate) active_scan_task_id: Option<String>,
    pub(crate) source_diagnostics: Vec<UsageSourceDiagnosticDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageDashboardDto {
    pub(crate) hero: UsageHeroOverviewDto,
    pub(crate) daily_trend: Vec<UsageDailyTrendBucketDto>,
    pub(crate) models: Vec<UsageModelBreakdownDto>,
    pub(crate) sources: Vec<UsageSourceBreakdownDto>,
    pub(crate) dates: Vec<UsageDateBreakdownDto>,
    pub(crate) scan_status: UsageScanStatusDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageDashboardFilter {
    #[serde(default)]
    pub(crate) time_range: Option<String>,
    #[serde(default)]
    pub(crate) adapter_id: Option<String>,
    #[serde(default)]
    pub(crate) source_id: Option<String>,
    #[serde(default)]
    pub(crate) model: Option<String>,
    #[serde(default)]
    pub(crate) timezone_offset_minutes: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageScanOptions {
    #[serde(default)]
    pub(crate) mode: Option<String>, // "incremental" or "full"
    #[serde(default)]
    pub(crate) source_id: Option<String>,
    #[serde(default)]
    pub(crate) adapter_id: Option<String>,
}
