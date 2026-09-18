use super::*;

#[test]
fn runtime_semver_preserves_minimum_language() {
    assert!(runtime_version_satisfies_constraint("v20.10.0", ">=20.2").unwrap());
    assert!(runtime_version_satisfies_constraint("Python 3.12.1", ">=3.12").unwrap());
    assert!(!runtime_version_satisfies_constraint("v18.19.0", ">=20").unwrap());
    assert!(runtime_version_satisfies_constraint("v20.0.0", ">=020").unwrap());
    assert!(validate_runtime_version_constraint("^20").is_err());
    assert!(validate_runtime_version_constraint(">=20.0.0.1").is_err());
}

#[test]
fn runtime_semver_uses_semver_and_no_manual_compare() {
    let source = include_str!("io_utils.rs");
    assert!(source.contains("semver::VersionReq"));
    assert!(!source.contains(concat!("fn ", "compare_versions")));
}
