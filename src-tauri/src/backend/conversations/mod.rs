mod external;
mod io_utils;
mod official;
mod prelude;
mod readers;
#[cfg(test)]
mod tests;
mod types;

pub(crate) use external::{
    adapter_from_registration_preview, register_external_adapter, scaffold_external_adapter,
    try_run_external_adapter, validate_external_adapter,
};
pub(crate) use official::ensure_official_conversation_adapters;
pub(crate) use readers::read_source_sessions_with_adapter;
#[allow(unused_imports)]
pub(crate) use types::{
    ConversationAdapterManifest, ExternalAdapterRegisterParams, ExternalAdapterRunResult,
    ExternalAdapterScaffoldParams, ExternalAdapterScaffoldResult, ExternalAdapterTryRunParams,
    ExternalAdapterValidateParams, ExternalAdapterValidationResult,
};
