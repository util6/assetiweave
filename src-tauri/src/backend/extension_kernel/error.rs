use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub(crate) enum ExtensionError {
    #[error("extension manifest is invalid for {package_id}: {reason}")]
    ManifestInvalid { package_id: String, reason: String },
    #[allow(dead_code)]
    #[error("extension is incompatible: {reason}")]
    Incompatible { package_id: String, reason: String },
    #[allow(dead_code)]
    #[error("extension trust was rejected: {reason}")]
    TrustRejected { package_id: String, reason: String },
    #[error("extension {package_id} failed to launch: {reason}")]
    LaunchFailed { package_id: String, reason: String },
    #[error("extension program was not found")]
    ProgramNotFound { package_id: String },
    #[error("extension {package_id} probe failed: {reason}")]
    ProbeFailed { package_id: String, reason: String },
    #[error("extension process timed out")]
    Timeout { package_id: String },
    #[error("extension process was cancelled")]
    Cancelled { package_id: String },
    #[error("extension process output exceeded the configured limit")]
    OutputLimitExceeded {
        package_id: String,
        stdout: bool,
        stderr: bool,
    },
    #[error("extension process exited unsuccessfully")]
    NonZeroExit {
        package_id: String,
        status: Option<i32>,
    },
    #[error("extension process cleanup failed: {reason}")]
    CleanupFailed { package_id: String, reason: String },
    #[error("extension lifecycle conflict: {0}")]
    Conflict(String),
}

impl ExtensionError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::ManifestInvalid { .. } => "manifest_invalid",
            Self::Incompatible { .. } => "incompatible",
            Self::TrustRejected { .. } => "trust_rejected",
            Self::ProgramNotFound { .. } => "program_not_found",
            Self::LaunchFailed { .. } => "launch_failed",
            Self::ProbeFailed { .. } => "probe_failed",
            Self::Timeout { .. } => "timeout",
            Self::Cancelled { .. } => "cancelled",
            Self::OutputLimitExceeded { .. } => "output_limit_exceeded",
            Self::NonZeroExit { .. } => "nonzero_exit",
            Self::CleanupFailed { .. } => "cleanup_failed",
            Self::Conflict(_) => "conflict",
        }
    }

    pub(crate) fn retryable(&self) -> bool {
        !matches!(
            self,
            Self::ManifestInvalid { .. }
                | Self::Incompatible { .. }
                | Self::TrustRejected { .. }
                | Self::ProgramNotFound { .. }
                | Self::OutputLimitExceeded { .. }
        )
    }

    pub(crate) fn details(&self) -> Option<serde_json::Value> {
        match self {
            Self::OutputLimitExceeded { stdout, stderr, .. } => {
                Some(serde_json::json!({ "stdout": stdout, "stderr": stderr }))
            }
            Self::NonZeroExit { status, .. } => Some(serde_json::json!({ "status": status })),
            _ => None,
        }
    }

    pub(crate) fn public_message(&self) -> String {
        match self {
            Self::ManifestInvalid { .. } => "extension manifest is invalid".to_string(),
            Self::Incompatible { .. } => "extension is incompatible".to_string(),
            Self::TrustRejected { .. } => "extension trust was rejected".to_string(),
            Self::ProgramNotFound { .. } => "extension program was not found".to_string(),
            Self::LaunchFailed { .. } => "extension process failed to launch".to_string(),
            Self::ProbeFailed { .. } => "extension probe failed".to_string(),
            Self::Timeout { .. } => "extension process timed out".to_string(),
            Self::Cancelled { .. } => "extension process was cancelled".to_string(),
            Self::OutputLimitExceeded { .. } => {
                "extension process output exceeded the configured limit".to_string()
            }
            Self::NonZeroExit { .. } => "extension process exited unsuccessfully".to_string(),
            Self::CleanupFailed { .. } => "extension process cleanup failed".to_string(),
            Self::Conflict(_) => {
                "extension lifecycle conflict: operation conflicts with an active lifecycle task"
                    .to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ExtensionError;

    #[test]
    fn domain_error_codes_cover_manifest_and_process_boundaries() {
        let errors = [
            (
                ExtensionError::ManifestInvalid {
                    package_id: "fixture".to_string(),
                    reason: "invalid".to_string(),
                },
                "manifest_invalid",
            ),
            (
                ExtensionError::Incompatible {
                    package_id: "fixture".to_string(),
                    reason: "version".to_string(),
                },
                "incompatible",
            ),
            (
                ExtensionError::TrustRejected {
                    package_id: "fixture".to_string(),
                    reason: "changed".to_string(),
                },
                "trust_rejected",
            ),
        ];

        for (error, code) in errors {
            assert_eq!(error.code(), code);
            assert!(!error.retryable());
            assert!(error.details().is_none());
        }
    }
}
