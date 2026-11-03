use super::*;
use crate::backend::domain::agents::Target;

#[test]
fn test_binary_materialize_file_with_valid_sha256() {
    let dir = tempfile_dir();
    let artifact_path = dir.join("agent.bin");
    let content = b"#!/bin/sh\necho opencode 1.0.0\n";
    std::fs::write(&artifact_path, content).unwrap();

    let expected_hash = hex_lower(&Sha256::digest(content));

    let distribution = Distribution::Binary {
        id: "binary-test".to_string(),
        priority: 10,
        target: Target {
            os: "darwin".to_string(),
            arch: "aarch64".to_string(),
        },
        archive: "none".to_string(),
        url: "https://example.com/agent.bin".to_string(),
        sha256: expected_hash.clone(),
        size: Some(content.len() as u64),
        executable: "agent".to_string(),
        launch_args: vec!["acp".to_string()],
        model_discovery_args: None,
        session_cleanup_args: None,
        session_cleanup_not_found_markers: Vec::new(),
    };

    let context = InstallContext::new(dir.join("staging"), "1.0.0");
    let runtime = BinaryInstaller
        .materialize_file(&distribution, &context, &artifact_path)
        .expect("materialize should succeed with matching sha256");

    assert_eq!(runtime.resolved_program, dir.join("staging").join("agent"));
    let integrity = runtime.integrity.expect("integrity json");
    assert_eq!(integrity["sha256"], expected_hash);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_binary_materialize_file_with_mismatched_sha256_fails() {
    let dir = tempfile_dir();
    let artifact_path = dir.join("agent.bin");
    let content = b"#!/bin/sh\necho opencode 1.0.0\n";
    std::fs::write(&artifact_path, content).unwrap();

    let distribution = Distribution::Binary {
        id: "binary-test".to_string(),
        priority: 10,
        target: Target {
            os: "darwin".to_string(),
            arch: "aarch64".to_string(),
        },
        archive: "none".to_string(),
        url: "https://example.com/agent.bin".to_string(),
        sha256: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        size: Some(content.len() as u64),
        executable: "agent".to_string(),
        launch_args: vec!["acp".to_string()],
        model_discovery_args: None,
        session_cleanup_args: None,
        session_cleanup_not_found_markers: Vec::new(),
    };

    let context = InstallContext::new(dir.join("staging"), "1.0.0");
    let err = BinaryInstaller
        .materialize_file(&distribution, &context, &artifact_path)
        .expect_err("materialize should fail on mismatched sha256");

    assert!(matches!(err, InstallError::IntegrityMismatch));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_binary_materialize_file_with_empty_sha256_exempts_and_records_actual_digest() {
    let dir = tempfile_dir();
    let artifact_path = dir.join("agent.bin");
    let content = b"#!/bin/sh\necho opencode 1.18.20\n";
    std::fs::write(&artifact_path, content).unwrap();

    let actual_hash = hex_lower(&Sha256::digest(content));

    // Dynamic version upgrade: sha256 is empty and size is None
    let distribution = Distribution::Binary {
        id: "binary-test".to_string(),
        priority: 10,
        target: Target {
            os: "darwin".to_string(),
            arch: "aarch64".to_string(),
        },
        archive: "none".to_string(),
        url: "https://example.com/agent-1.18.20.bin".to_string(),
        sha256: String::new(),
        size: None,
        executable: "agent".to_string(),
        launch_args: vec!["acp".to_string()],
        model_discovery_args: None,
        session_cleanup_args: None,
        session_cleanup_not_found_markers: Vec::new(),
    };

    let context = InstallContext::new(dir.join("staging"), "1.18.20");
    let runtime = BinaryInstaller
        .materialize_file(&distribution, &context, &artifact_path)
        .expect("materialize should succeed when sha256 is empty for dynamic version");

    assert_eq!(runtime.resolved_program, dir.join("staging").join("agent"));
    let integrity = runtime.integrity.expect("integrity json");
    assert_eq!(integrity["sha256"], actual_hash);
    assert_eq!(integrity["size"], content.len() as u64);
    let _ = std::fs::remove_dir_all(&dir);
}

fn tempfile_dir() -> PathBuf {
    let base = std::env::temp_dir()
        .canonicalize()
        .unwrap_or_else(|_| std::env::temp_dir());
    let path = base.join(format!(
        "assetiweave-binary-installer-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
