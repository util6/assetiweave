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
    #[serde(default)]
    pub unit_price_per_million: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelPricingSyncResultDto {
    pub channel_used: String,
    pub models_count: usize,
    pub exchange_rate: f64,
    pub recalculated_events_count: usize,
    pub catalog_version: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelPricingStatusDto {
    pub models_count: usize,
    #[serde(default)]
    pub local_models_count: usize,
    pub exchange_rate: f64,
    pub active_channel: String,
    pub catalog_version: String,
    pub last_updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelPriceEntryDto {
    pub provider: String,
    pub model_patterns: Vec<String>,
    pub currency: String,
    pub input_per_million: f64,
    pub cache_read_per_million: f64,
    pub cache_write_per_million: f64,
    pub output_per_million: f64,
    pub reasoning_per_million: f64,
    pub raw_currency: Option<String>,
    pub raw_input_per_million: Option<f64>,
    pub raw_output_per_million: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelPricingCatalogDto {
    pub catalog_version: String,
    pub updated_at: String,
    pub usd_to_cny_rate: f64,
    pub active_channel: String,
    pub models: Vec<ModelPriceEntryDto>,
    #[serde(default)]
    pub local_models: Vec<ModelPriceEntryDto>,
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
pub struct UsageProjectBreakdownDto {
    pub project_path: String,
    pub display_name: String,
    pub is_unassigned: bool,
    pub requests_count: i64,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_tokens: i64,
    pub costs_by_currency: Vec<CurrencyCostDto>,
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
    pub projects: Vec<UsageProjectBreakdownDto>,
    pub dates: Vec<UsageDateBreakdownDto>,
    pub scan_status: UsageScanStatusDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageDashboardFilter {
    #[serde(default)]
    pub time_range: Option<String>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default)]
    pub adapter_id: Option<String>,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub project_path: Option<String>,
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
