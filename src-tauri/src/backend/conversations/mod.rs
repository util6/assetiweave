pub(crate) mod cards;
pub(crate) const CONVERSATION_PAYLOAD_POLICY_VERSION: u32 = 10;
mod external;
mod harvester;
mod io_utils;
mod official;
mod package;
mod prelude;
mod readers;
#[cfg(test)]
mod tests;
mod types;

pub(crate) use external::{
    adapter_from_registration_preview, export_external_adapter_markdown_with_settings,
    list_conversation_adapter_runtime_statuses_with_settings,
    register_external_adapter_with_settings, scaffold_external_adapter,
    try_run_external_adapter_with_settings, validate_external_adapter,
};
#[cfg(test)]
pub(crate) use external::{
    list_conversation_adapter_runtime_statuses, register_external_adapter, try_run_external_adapter,
};
pub(crate) use harvester::run_conversation_harvester_for_adapter_source_with_settings;
#[cfg(test)]
pub(crate) use harvester::{
    run_conversation_harvester_for_adapter_source, run_conversation_harvester_for_source,
};
pub(crate) use official::ensure_official_conversation_adapters;
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
    read_source_sessions_with_adapter_with_settings, ConversationSourceReadResult,
};
#[allow(unused_imports)]
pub(crate) use types::{
    ConversationAdapterCatalog, ConversationAdapterManifest, ConversationAdapterRuntimeKind,
    ConversationAdapterRuntimeStatus, ConversationSessionDescriptor, ExternalAdapterRegisterParams,
    ExternalAdapterRunResult, ExternalAdapterScaffoldParams, ExternalAdapterScaffoldResult,
    ExternalAdapterTryRunParams, ExternalAdapterValidateParams, ExternalAdapterValidationResult,
};
