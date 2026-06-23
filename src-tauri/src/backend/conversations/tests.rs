use super::prelude::*;
use super::{
    read_source_sessions_with_adapter, try_run_external_adapter, validate_external_adapter,
};

struct TempFixture {
    path: PathBuf,
}

impl TempFixture {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn adapter_output_rejects_oversized_line() {
    let line = format!(
        "{{\"type\":\"warning\",\"message\":\"{}\"}}\n{{\"type\":\"complete\",\"item\":{{}}}}\n",
        "x".repeat(DEFAULT_MAX_LINE_BYTES + 1)
    );
    let error = parse_external_adapter_output("probe", line.into_bytes(), Vec::new()).unwrap_err();

    assert!(error.contains("exceeds max line size"));
}

#[test]
fn adapter_output_requires_complete_line() {
    let output =
        br#"{"type":"item","item":{"kind":"session","session":{"external_id":"s","title":null,"project_path":null,"started_at":null,"updated_at":null,"source_locator":null,"source_fingerprint":null,"turns":[]}}}"#;
    let error = parse_external_adapter_output("read_session", output.to_vec(), Vec::new())
        .expect_err("missing complete line should fail");

    assert!(error.contains("complete"));
}

#[test]
fn external_adapter_validation_hash_changes_when_executable_changes() {
    let fixture = TempFixture::new("assetiweave-adapter-validation-fixture");
    let script = write_executable_script(
        fixture.path(),
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"complete","item":{}}'
"#,
    );
    let manifest = write_manifest(fixture.path(), vec!["adapter.sh".to_string()]);
    let before = validate_external_adapter(ExternalAdapterValidateParams {
        manifest_path: manifest.to_string_lossy().to_string(),
    })
    .unwrap();

    fs::write(
        &script,
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"warning","message":"changed"}'
printf '%s\n' '{"type":"complete","item":{}}'
"#,
    )
    .unwrap();
    let after = validate_external_adapter(ExternalAdapterValidateParams {
        manifest_path: manifest.to_string_lossy().to_string(),
    })
    .unwrap();

    assert_eq!(before.executable_path, script.to_string_lossy());
    assert_ne!(before.executable_hash, after.executable_hash);
}

#[cfg(unix)]
#[test]
fn external_adapter_try_run_parses_sessions_without_shell_joining_args() {
    let fixture = TempFixture::new("assetiweave-adapter-run-fixture");
    let hacked_path = fixture.path().join("hacked");
    write_executable_script(
        fixture.path(),
        "adapter.sh",
        r#"#!/bin/sh
printf '%s\n' "$1" >&2
cat >/dev/null
printf '%s\n' '{"type":"item","item":{"kind":"session","session":{"external_id":"external-session-1","title":"External Fixture","project_path":null,"started_at":null,"updated_at":null,"source_locator":null,"source_fingerprint":null,"turns":[{"external_id":"turn-1","turn_index":0,"user_text":"External question","title":null,"started_at":null,"ended_at":null,"parts":[{"role":"assistant","kind":"text","text":"External answer","language":null,"command":null,"cwd":null,"status":null,"exit_code":null,"metadata_json":{"content_card":{"type":"answer","format":"markdown"}}}]}]}}}'
printf '%s\n' '{"type":"warning","message":"fixture warning"}'
printf '%s\n' '{"type":"complete","item":{"session_count":1}}'
"#,
    );
    let injection_arg = format!("literal; touch {}", hacked_path.display());
    let manifest = write_manifest(
        fixture.path(),
        vec!["adapter.sh".to_string(), injection_arg.clone()],
    );

    let result = try_run_external_adapter(ExternalAdapterTryRunParams {
        manifest_path: manifest.to_string_lossy().to_string(),
        method: "read_session".to_string(),
        location: Some(fixture.path().to_string_lossy().to_string()),
        session_id: Some("external-session-1".to_string()),
        yes: true,
    })
    .unwrap();

    assert_eq!(result.item_count, 1);
    assert_eq!(result.warning_count, 1);
    assert_eq!(result.sessions[0].turns[0].user_text, "External question");
    assert!(result.stderr.contains(&injection_arg));
    assert!(!hacked_path.exists());
}

#[cfg(unix)]
#[test]
fn external_adapter_sync_reads_registered_adapter_sessions() {
    let fixture = TempFixture::new("assetiweave-adapter-sync-fixture");
    write_executable_script(
        fixture.path(),
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"item","item":{"kind":"session","session":{"external_id":"web-session-1","title":"Web Fixture","project_path":null,"started_at":null,"updated_at":null,"source_locator":"fixture://web-session-1","source_fingerprint":"fixture-hash","turns":[{"external_id":"turn-1","turn_index":0,"user_text":"Web question","title":null,"started_at":null,"ended_at":null,"parts":[{"role":"assistant","kind":"text","text":"Web answer","language":null,"command":null,"cwd":null,"status":null,"exit_code":null,"metadata_json":{"content_card":{"type":"answer","format":"markdown"}}}]}]}}}'
printf '%s\n' '{"type":"complete","item":{"session_count":1}}'
"#,
    );
    let manifest = write_manifest(fixture.path(), vec!["adapter.sh".to_string()]);
    let adapter = ConversationAdapter {
        id: "fixture-external".to_string(),
        name: "Fixture External".to_string(),
        kind: ConversationAdapterKind::External,
        version: "0.1.0".to_string(),
        enabled: true,
        manifest_path: Some(manifest.to_string_lossy().to_string()),
        executable_path: Some(
            fixture
                .path()
                .join("adapter.sh")
                .to_string_lossy()
                .to_string(),
        ),
        content_hash: None,
        trusted_hash: None,
        trust_state: ConversationAdapterTrustState::Trusted,
        protocol_version: Some(EXTERNAL_ADAPTER_PROTOCOL_VERSION),
        capabilities: vec!["read_session".to_string()],
        input_kinds: vec![ConversationSourceKind::Directory],
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    };
    let source = source_fixture(
        "fixture-external",
        ConversationSourceKind::Directory,
        &fixture.path().to_string_lossy(),
    );

    let sessions = read_source_sessions_with_adapter(Some(&adapter), &source).unwrap();

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].external_id, "web-session-1");
    assert_eq!(sessions[0].turns[0].user_text, "Web question");
    assert_eq!(
        sessions[0].turns[0].parts[0].text.as_deref(),
        Some("Web answer")
    );
}

#[cfg(unix)]
#[test]
fn external_adapter_run_times_out() {
    let fixture = TempFixture::new("assetiweave-adapter-timeout-fixture");
    write_executable_script(
        fixture.path(),
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
sleep 1
printf '%s\n' '{"type":"complete","item":{}}'
"#,
    );
    let manifest = write_manifest(fixture.path(), vec!["adapter.sh".to_string()]);
    let validation =
        validate_external_adapter_manifest(manifest.to_string_lossy().as_ref()).unwrap();

    let error = run_external_adapter(
        &validation,
        "probe",
        json!({"method":"probe"}),
        Duration::from_millis(50),
    )
    .unwrap_err();

    assert!(error.contains("timed out"));
}

fn source_fixture(
    adapter_id: &str,
    kind: ConversationSourceKind,
    location: &str,
) -> ConversationSource {
    ConversationSource {
        id: format!("{adapter_id}-fixture"),
        adapter_id: adapter_id.to_string(),
        name: format!("{adapter_id} fixture"),
        kind,
        location: location.to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

#[cfg(unix)]
fn write_executable_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join(name);
    fs::write(&path, body).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

#[cfg(unix)]
fn write_manifest(dir: &Path, command: Vec<String>) -> PathBuf {
    let manifest = ConversationAdapterManifest {
        schema_version: 1,
        id: "fixture-external".to_string(),
        name: "Fixture External".to_string(),
        version: "0.1.0".to_string(),
        protocol_version: EXTERNAL_ADAPTER_PROTOCOL_VERSION,
        command,
        capabilities: vec![
            "probe".to_string(),
            "list_sessions".to_string(),
            "read_session".to_string(),
        ],
        input_kinds: vec![ConversationSourceKind::Directory],
    };
    let path = dir.join("conversation-adapter.json");
    fs::write(&path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
    path
}
