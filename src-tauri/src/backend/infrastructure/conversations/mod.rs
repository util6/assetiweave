pub(crate) const CONVERSATION_PAYLOAD_POLICY_VERSION: u32 = 10;
pub(crate) mod external;
pub(crate) mod external_manifest;
pub(crate) mod external_parser;
pub(crate) mod external_process;
pub(crate) mod external_reader;
pub(crate) mod external_sanitize;
pub(crate) mod external_scaffold;
pub(crate) mod harvester;
pub(crate) mod io_utils;
pub(crate) mod official;
pub(crate) mod package;
#[cfg(test)]
#[path = "path_normalization_tests.rs"]
mod path_normalization_tests;
pub(crate) mod prelude;
pub(crate) mod readers;
pub(crate) mod runtime_probe;
pub(crate) mod runtime_requirements;
#[cfg(test)]
mod tests;
pub(crate) mod types;

pub(crate) use external::{
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
pub(crate) use external::{register_external_adapter, try_run_external_adapter};
pub(crate) use harvester::run_conversation_harvester_with_control;
pub(crate) use official::{
    ensure_official_conversation_adapters, ensure_shell_command_projector, is_official_adapter_id,
    sync_official_adapter_to_package_dir,
};
pub(crate) use package::{
    validate_conversation_adapter_package_dir, ConversationAdapterPackageInstallSource,
    ConversationAdapterPackageInstallSourceKind, ConversationAdapterPackageInstallSpec,
    ConversationAdapterPackageRuntimeProtocol, ConversationAdapterPackageSystem,
    ConversationAdapterPackageValidationResult,
};
#[cfg(test)]
pub(crate) use readers::{
    read_source_sessions_incrementally_with_adapter, read_source_sessions_with_adapter,
};
#[allow(unused_imports)]
pub(crate) use readers::{
    read_source_sessions_incrementally_with_adapter_with_settings,
    read_source_sessions_with_adapter_with_settings, read_source_sessions_with_control,
    read_source_sessions_with_progress_listener, ConversationSourceReadResult,
};
#[allow(unused_imports)]
pub(crate) use types::{
    ConversationAdapterManifest, ConversationAdapterRuntimeKind, ConversationAdapterRuntimeStatus,
    ConversationCommandProjection, ConversationCommandProjectionParams,
    ConversationCommandProjectionPart, ConversationSessionDescriptor, ExternalAdapterProgress,
    ExternalAdapterRegisterParams, ExternalAdapterRunResult, ExternalAdapterScaffoldParams,
    ExternalAdapterScaffoldResult, ExternalAdapterTryRunParams, ExternalAdapterValidateParams,
    ExternalAdapterValidationResult,
};
