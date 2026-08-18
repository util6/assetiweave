use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub(crate) enum ExtensionError {
    #[error("extension manifest is invalid for {package_id}: {reason}")]
    ManifestInvalid { package_id: String, reason: String },
    #[error("extension {package_id} is incompatible: need {need}, host has {have}")]
    Incompatible {
        package_id: String,
        need: String,
        have: String,
    },
    #[error("extension {package_id} trust state is rejected: {state}")]
    TrustRejected { package_id: String, state: String },
    #[error("extension {package_id} failed to launch: {reason}")]
    LaunchFailed { package_id: String, reason: String },
    #[error("extension {package_id} probe failed: {reason}")]
    ProbeFailed { package_id: String, reason: String },
    #[error("extension lifecycle conflict: {0}")]
    Conflict(String),
}
