use super::*;

#[tokio::test]
#[cfg(unix)]
async fn system_materialization_never_claims_an_owned_directory() {
    let directory = tempfile_dir();
    let executable = directory.join("agent");
    std::fs::write(&executable, b"#!/bin/sh\necho agent 1.0.0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let installer = SystemInstaller {
        resolver: Some(executable.clone()),
    };
    let context = InstallContext::new(directory.join("staging"), "1.0.0");
    let distribution = Distribution::System {
        id: "system".to_string(),
        priority: 10,
        command_candidates: vec!["agent".to_string()],
        version_args: vec![],
        version_range: ">=1.0.0".to_string(),
        launch_args: vec!["acp".to_string()],
        model_discovery_args: None,
        session_cleanup_args: None,
        session_cleanup_not_found_markers: Vec::new(),
    };
    let runtime = installer
        .materialize(&distribution, &context)
        .await
        .unwrap();
    assert_eq!(runtime.ownership, Ownership::System);
    assert!(runtime.install_dir.is_none());
    assert_eq!(runtime.resolved_program, executable);
}

fn tempfile_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-system-installer-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
