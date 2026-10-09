pub const CONVERSATION_PAYLOAD_POLICY_VERSION: u32 = 10;
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
pub mod package;
#[cfg(test)]
#[path = "path_normalization_tests.rs"]
mod path_normalization_tests;
pub mod prelude;
pub mod pricing_fetcher;
pub mod readers;
pub mod runtime_probe;
pub mod runtime_requirements;
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
    ensure_official_conversation_adapters, ensure_shell_command_projector, is_official_adapter_id,
    sync_official_adapter_to_package_dir,
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
