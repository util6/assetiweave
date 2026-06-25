mod external;
mod harvester;
mod io_utils;
mod official;
mod prelude;
mod readers;
#[cfg(test)]
mod tests;
mod types;

pub(crate) use external::{
    adapter_from_registration_preview, export_external_adapter_markdown,
    list_conversation_adapter_runtime_statuses, register_external_adapter,
    scaffold_external_adapter, try_run_external_adapter, validate_external_adapter,
};
pub(crate) use harvester::run_conversation_harvester_for_source;
pub(crate) use official::ensure_official_conversation_adapters;
pub(crate) use readers::read_source_sessions_with_adapter;
#[allow(unused_imports)]
pub(crate) use types::{
    ConversationAdapterManifest, ConversationAdapterRuntimeKind, ConversationAdapterRuntimeStatus,
    ExternalAdapterRegisterParams, ExternalAdapterRunResult, ExternalAdapterScaffoldParams,
    ExternalAdapterScaffoldResult, ExternalAdapterTryRunParams, ExternalAdapterValidateParams,
    ExternalAdapterValidationResult,
};
