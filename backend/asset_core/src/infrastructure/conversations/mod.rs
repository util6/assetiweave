/// 会话载荷解析策略版本。
///
/// 该值参与增量同步的水位身份判定：`conversation_sources` 上记录的
/// `synced_payload_policy_version` 与它不一致时，该数据源的全部会话水位会被清空，
/// 下一轮同步重新读取并解析存量会话；同时 `conversation_payload_policy_state`
/// 里记录的已应用版本落后时，启动阶段会主动请求一次全量重解析。
///
/// 提升该值即可驱动存量会话重解析，禁止另行引入第二套版本机制。
///
/// - v10：子代理 Turn 建模（Issue #148）之前的载荷策略。
/// - v11：新增子代理 Turn 树、真实角色与派发提示词，存量会话必须重解析补齐结构。
pub const CONVERSATION_PAYLOAD_POLICY_VERSION: u32 = 13;
pub mod external;
pub mod external_manifest;
pub mod external_parser;
pub mod external_process;
pub mod external_reader;
pub mod external_sanitize;
pub mod external_scaffold;
pub mod harvester;
pub mod io_utils;
pub mod official;
pub mod official_runtime;
pub mod package;
#[cfg(test)]
#[path = "path_normalization_tests.rs"]
mod path_normalization_tests;
pub mod prelude;
pub mod pricing_fetcher;
mod pricing_target_filter;
pub mod readers;
pub mod runtime_probe;
pub mod runtime_requirements;
#[cfg(test)]
pub mod test_fixtures;
#[cfg(test)]
mod tests;
pub mod types;

pub use external::{
    adapter_from_registration_preview, adapter_supports_usage,
    export_external_adapter_markdown_with_settings,
    list_conversation_adapter_runtime_statuses_with_settings,
    project_external_adapter_command_parts_with_settings,
    read_external_adapter_usage_with_settings, register_external_adapter_with_settings,
    run_external_adapter_read_session, scaffold_external_adapter,
    try_run_external_adapter_with_settings, validate_external_adapter,
    ExternalAdapterProgressListener,
};
#[cfg(test)]
pub use external::{register_external_adapter, try_run_external_adapter};
pub use harvester::run_conversation_harvester_with_control;
pub use official::{
    discover_managed_conversation_adapters, discover_managed_conversation_adapters_at,
};
pub use official_runtime::{
    ensure_reproject_runner_scripts, ensure_shell_command_projector, run_reproject_runner,
};
pub use package::{
    validate_conversation_adapter_package_dir, ConversationAdapterPackageInstallSource,
    ConversationAdapterPackageInstallSourceKind, ConversationAdapterPackageInstallSpec,
    ConversationAdapterPackageRuntimeProtocol, ConversationAdapterPackageSystem,
    ConversationAdapterPackageValidationResult,
};
pub use pricing_fetcher::{
    fetch_tiered_pricing_catalog, fetch_usd_to_cny_rate, PricingSourceChannel, TieredFetchOutcome,
};
#[cfg(test)]
pub use readers::{
    read_source_sessions_incrementally_with_adapter, read_source_sessions_with_adapter,
};
#[allow(unused_imports)]
pub use readers::{
    read_source_sessions_incrementally_with_adapter_with_settings,
    read_source_sessions_with_adapter_with_settings, read_source_sessions_with_control,
    read_source_sessions_with_progress_listener, ConversationSourceReadResult,
};
#[allow(unused_imports)]
pub use types::{
    ConversationAdapterManifest, ConversationAdapterRuntimeKind, ConversationAdapterRuntimeStatus,
    ConversationCommandProjection, ConversationCommandProjectionParams,
    ConversationCommandProjectionPart, ConversationSessionDescriptor, ExternalAdapterProgress,
    ExternalAdapterRegisterParams, ExternalAdapterRunResult, ExternalAdapterScaffoldParams,
    ExternalAdapterScaffoldResult, ExternalAdapterTryRunParams, ExternalAdapterValidateParams,
    ExternalAdapterValidationResult,
};
