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
