use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CurrencyCostDto {
    pub currency: String,
    pub amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageDailyTrendBucketDto {
    pub date: String,
    pub total_tokens: i64,
    pub requests_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_tokens: i64,
    pub costs_by_currency: Vec<CurrencyCostDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageModelBreakdownDto {
    pub model: String,
    pub provider: String,
    pub requests_count: i64,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub cache_tokens: i64,
    pub cost: Option<f64>,
    pub currency: String,
    pub cost_basis: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageSourceBreakdownDto {
    pub adapter_id: String,
    pub source_id: String,
    pub source_name: String,
    pub app_name: String,
    pub requests_count: i64,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_tokens: i64,
    pub costs_by_currency: Vec<CurrencyCostDto>,
    pub status: String,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageDateBreakdownDto {
    pub date: String,
    pub requests_count: i64,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_tokens: i64,
    pub costs_by_currency: Vec<CurrencyCostDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageSourceDiagnosticDto {
    pub source_id: String,
    pub adapter_id: String,
    pub level: String,
    pub message: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageHeroOverviewDto {
    pub total_tokens: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub reasoning_tokens: i64,
    pub requests_count: i64,
    pub costs_by_currency: Vec<CurrencyCostDto>,
    pub cost_covered_tokens: i64,
    pub cost_coverage_ratio: f64,
    pub cost_covered_requests: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageScanStatusDto {
    pub scanned_sources_count: usize,
    pub total_events_count: usize,
    pub last_scanned_at: Option<String>,
    pub active_scan_task_id: Option<String>,
    pub source_diagnostics: Vec<UsageSourceDiagnosticDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageDashboardDto {
    pub hero: UsageHeroOverviewDto,
    pub daily_trend: Vec<UsageDailyTrendBucketDto>,
    pub models: Vec<UsageModelBreakdownDto>,
    pub sources: Vec<UsageSourceBreakdownDto>,
    pub dates: Vec<UsageDateBreakdownDto>,
    pub scan_status: UsageScanStatusDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageDashboardFilter {
    #[serde(default)]
    pub time_range: Option<String>,
    #[serde(default)]
    pub adapter_id: Option<String>,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub timezone_offset_minutes: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageScanOptions {
    #[serde(default)]
    pub mode: Option<String>, // "incremental" or "full"
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub adapter_id: Option<String>,
}
