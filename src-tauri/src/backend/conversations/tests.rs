use super::prelude::*;
use super::{
    read_source_sessions_with_adapter, scaffold_external_adapter, try_run_external_adapter,
    validate_external_adapter,
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
fn adapter_output_parses_markdown_export_item() {
    let output = br##"{"type":"item","item":{"kind":"markdown_export","content":"# Exported","relative_path":"codex/project/session.md"}}
{"type":"complete","item":{"export_count":1}}"##;

    let result =
        parse_external_adapter_output("export_markdown", output.to_vec(), Vec::new()).unwrap();

    let export = result.markdown_export.expect("markdown export item");
    assert_eq!(result.item_count, 1);
    assert_eq!(export.content, "# Exported");
    assert_eq!(export.relative_path, "codex/project/session.md");
}

#[test]
fn adapter_output_rejects_empty_markdown_export_content() {
    let output = br#"{"type":"item","item":{"kind":"markdown_export","content":"","relative_path":"codex/project/session.md"}}
{"type":"complete","item":{"export_count":1}}"#;

    let error = parse_external_adapter_output("export_markdown", output.to_vec(), Vec::new())
        .expect_err("empty export content should fail");

    assert!(error.contains("content"));
}

#[test]
fn adapter_output_rejects_missing_markdown_export_fields() {
    let missing_content = br#"{"type":"item","item":{"kind":"markdown_export","relative_path":"codex/project/session.md"}}
{"type":"complete","item":{"export_count":1}}"#;
    let error =
        parse_external_adapter_output("export_markdown", missing_content.to_vec(), Vec::new())
            .expect_err("missing export content should fail");
    assert!(error.contains("content"));

    let missing_relative_path =
        br##"{"type":"item","item":{"kind":"markdown_export","content":"# Exported"}}
{"type":"complete","item":{"export_count":1}}"##;
    let error = parse_external_adapter_output(
        "export_markdown",
        missing_relative_path.to_vec(),
        Vec::new(),
    )
    .expect_err("missing export relative_path should fail");
    assert!(error.contains("relative_path"));
}

#[test]
fn adapter_command_invocation_runs_javascript_adapters_through_node() {
    let manifest_dir = Path::new("/tmp/adapter");
    let invocation =
        build_adapter_command_invocation(manifest_dir, "adapter.mjs", &["--probe".to_string()]);

    assert_eq!(invocation.program, PathBuf::from("node"));
    assert_eq!(
        invocation.args,
        vec![
            manifest_dir
                .join("adapter.mjs")
                .to_string_lossy()
                .to_string(),
            "--probe".to_string()
        ]
    );
    assert_eq!(invocation.display_path, manifest_dir.join("adapter.mjs"));
}

#[test]
fn adapter_command_invocation_treats_javascript_extensions_case_insensitively() {
    let manifest_dir = Path::new("/tmp/adapter");
    let invocation = build_adapter_command_invocation(manifest_dir, "adapter.MJS", &[]);

    assert_eq!(invocation.program, PathBuf::from("node"));
    assert_eq!(
        invocation.args,
        vec![manifest_dir
            .join("adapter.MJS")
            .to_string_lossy()
            .to_string()]
    );
}

#[test]
fn adapter_runtime_invocation_uses_declared_node_runtime() {
    let manifest_dir = Path::new("/tmp/adapter");
    let runtime = ConversationAdapterRuntime {
        kind: ConversationAdapterRuntimeKind::Node,
        entry: "adapter.mjs".to_string(),
        args: vec!["--mode".to_string(), "probe".to_string()],
        version: Some(">=20".to_string()),
    };

    let invocation =
        build_adapter_runtime_invocation(manifest_dir, &runtime, &["--source".to_string()]);

    assert_eq!(invocation.program, PathBuf::from("node"));
    assert_eq!(
        invocation.args,
        vec![
            manifest_dir
                .join("adapter.mjs")
                .to_string_lossy()
                .to_string(),
            "--mode".to_string(),
            "probe".to_string(),
            "--source".to_string()
        ]
    );
    assert_eq!(invocation.display_path, manifest_dir.join("adapter.mjs"));
}

#[test]
fn adapter_runtime_invocation_supports_python_and_bash() {
    let manifest_dir = Path::new("/tmp/adapter");
    let python = ConversationAdapterRuntime {
        kind: ConversationAdapterRuntimeKind::Python,
        entry: "adapter.py".to_string(),
        args: Vec::new(),
        version: Some(">=3.10".to_string()),
    };
    let bash = ConversationAdapterRuntime {
        kind: ConversationAdapterRuntimeKind::Bash,
        entry: "adapter.sh".to_string(),
        args: Vec::new(),
        version: None,
    };

    let python_invocation = build_adapter_runtime_invocation(manifest_dir, &python, &[]);
    let bash_invocation = build_adapter_runtime_invocation(manifest_dir, &bash, &[]);

    #[cfg(windows)]
    {
        assert_eq!(python_invocation.program, PathBuf::from("py"));
        assert_eq!(python_invocation.args[0], "-3");
        assert_eq!(
            python_invocation.args[1],
            manifest_dir
                .join("adapter.py")
                .to_string_lossy()
                .to_string()
        );
    }
    #[cfg(not(windows))]
    {
        assert_eq!(python_invocation.program, PathBuf::from("python3"));
        assert_eq!(
            python_invocation.args[0],
            manifest_dir
                .join("adapter.py")
                .to_string_lossy()
                .to_string()
        );
    }
    assert_eq!(bash_invocation.program, PathBuf::from("bash"));
    assert_eq!(
        bash_invocation.args[0],
        manifest_dir
            .join("adapter.sh")
            .to_string_lossy()
            .to_string()
    );
}

#[test]
fn adapter_runtime_probe_reports_missing_system_runtime() {
    let runtime = ConversationAdapterRuntime {
        kind: ConversationAdapterRuntimeKind::Node,
        entry: "adapter.mjs".to_string(),
        args: Vec::new(),
        version: Some(">=20".to_string()),
    };
    let invocation = AdapterCommandInvocation {
        program: PathBuf::from("assetiweave-missing-node-runtime"),
        args: Vec::new(),
        display_path: PathBuf::from("adapter.mjs"),
    };

    let error = ensure_adapter_runtime_available(&runtime, &invocation).unwrap_err();

    assert!(error.contains("node >=20"));
    assert!(error.contains("PATH"));
    assert!(error.contains("assetiweave-missing-node-runtime"));
}

#[cfg(unix)]
#[test]
fn adapter_runtime_probe_rejects_detected_version_below_requirement() {
    let fixture = TempFixture::new("assetiweave-runtime-version-fixture");
    let runtime_program = write_executable_script(
        fixture.path(),
        "node18.sh",
        r#"#!/bin/sh
printf '%s\n' 'v18.19.0'
"#,
    );
    let runtime = ConversationAdapterRuntime {
        kind: ConversationAdapterRuntimeKind::Node,
        entry: "adapter.mjs".to_string(),
        args: Vec::new(),
        version: Some(">=20".to_string()),
    };
    let invocation = AdapterCommandInvocation {
        program: runtime_program,
        args: Vec::new(),
        display_path: PathBuf::from("adapter.mjs"),
    };

    let error = ensure_adapter_runtime_available(&runtime, &invocation).unwrap_err();

    assert!(error.contains("requires >=20"));
    assert!(error.contains("v18.19.0"));
}

#[cfg(unix)]
#[test]
fn adapter_runtime_status_reports_version_requirement_mismatch() {
    let fixture = TempFixture::new("assetiweave-runtime-status-version-fixture");
    let runtime_program = write_executable_script(
        fixture.path(),
        "node18.sh",
        r#"#!/bin/sh
printf '%s\n' 'v18.19.0'
"#,
    );

    let status = probe_adapter_runtime_status_with_requirement(
        &ConversationAdapterRuntimeKind::Node,
        runtime_program,
        Some(">=20"),
    );

    assert!(!status.available);
    assert_eq!(status.version.as_deref(), Some("v18.19.0"));
    assert_eq!(status.required_version.as_deref(), Some(">=20"));
    assert!(status
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("requires >=20"));
}

#[test]
fn adapter_runtime_probe_returns_remediation_hint() {
    let status = probe_adapter_runtime_status(
        &ConversationAdapterRuntimeKind::Node,
        PathBuf::from("assetiweave-missing-node-runtime"),
    );

    assert!(!status.available);
    assert!(status
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("not found"));
    assert!(status
        .hint
        .as_deref()
        .unwrap_or_default()
        .contains("Node.js 20"));
    assert!(status.hint.as_deref().unwrap_or_default().contains("PATH"));
}

#[test]
fn adapter_runtime_version_constraints_compare_detected_versions() {
    assert!(runtime_version_satisfies_constraint("v20.11.1", ">=20").unwrap());
    assert!(runtime_version_satisfies_constraint("Python 3.11.6", ">=3.10").unwrap());
    assert!(
        runtime_version_satisfies_constraint("GNU bash, version 5.2.37(1)-release", ">=5.2")
            .unwrap()
    );
    assert!(!runtime_version_satisfies_constraint("v18.19.0", ">=20").unwrap());
    assert!(!runtime_version_satisfies_constraint("Python 3.9.18", ">=3.10").unwrap());
}

#[test]
fn adapter_runtime_version_constraints_reject_unsupported_shapes() {
    assert!(runtime_version_satisfies_constraint("v20.11.1", "^20").is_err());
    assert!(runtime_version_satisfies_constraint("v20.11.1", ">=20.x").is_err());
    assert!(runtime_version_satisfies_constraint("node version unknown", ">=20").is_err());
}

#[test]
fn adapter_runtime_status_lists_supported_system_runtimes() {
    let statuses = list_adapter_runtime_statuses();
    let kinds = statuses
        .iter()
        .map(|status| status.kind.clone())
        .collect::<Vec<_>>();

    assert_eq!(
        kinds,
        vec![
            ConversationAdapterRuntimeKind::Node,
            ConversationAdapterRuntimeKind::Python,
            ConversationAdapterRuntimeKind::Bash
        ]
    );
    assert!(statuses.iter().all(|status| !status.program.is_empty()));
    assert_eq!(
        statuses
            .iter()
            .find(|status| status.kind == ConversationAdapterRuntimeKind::Node)
            .and_then(|status| status.required_version.as_deref()),
        Some(">=20")
    );
}

#[test]
fn adapter_runtime_overrides_read_configured_programs() {
    let settings = json!({
        "conversationRuntimeOverrides": {
            "node": "/opt/node/bin/node",
            "python": "  /opt/python/bin/python3  ",
            "bash": "",
            "ignored": "/tmp/ignored"
        }
    });

    assert_eq!(
        runtime_program_from_settings(&ConversationAdapterRuntimeKind::Node, &settings),
        Some(PathBuf::from("/opt/node/bin/node"))
    );
    assert_eq!(
        runtime_program_from_settings(&ConversationAdapterRuntimeKind::Python, &settings),
        Some(PathBuf::from("/opt/python/bin/python3"))
    );
    assert_eq!(
        runtime_program_from_settings(&ConversationAdapterRuntimeKind::Bash, &settings),
        None
    );
    assert_eq!(
        runtime_program_from_settings(&ConversationAdapterRuntimeKind::Executable, &settings),
        None
    );
}

#[test]
fn external_adapter_validation_accepts_runtime_without_legacy_command() {
    let fixture = TempFixture::new("assetiweave-adapter-runtime-fixture");
    let adapter_path = fixture.path().join("adapter.mjs");
    fs::write(&adapter_path, "#!/usr/bin/env node\n").unwrap();
    let manifest_path = fixture.path().join("conversation-adapter.json");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "id": "fixture-runtime",
            "name": "Fixture Runtime",
            "version": "0.1.0",
            "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
            "runtime": {
                "type": "node",
                "entry": "adapter.mjs",
                "version": ">=20"
            },
            "capabilities": ["probe", "read_session", "export_markdown"],
            "input_kinds": ["directory"]
        }))
        .unwrap(),
    )
    .unwrap();

    let validation =
        validate_external_adapter_manifest(manifest_path.to_string_lossy().as_ref()).unwrap();

    assert_eq!(validation.executable_path, adapter_path.to_string_lossy());
    assert!(validation.executable_hash.is_some());
    assert!(validation.manifest.command.is_empty());
    assert_eq!(
        validation
            .manifest
            .runtime
            .as_ref()
            .map(|runtime| runtime.entry.as_str()),
        Some("adapter.mjs")
    );
}

#[test]
fn external_adapter_validation_rejects_unsupported_runtime_version_constraint() {
    let fixture = TempFixture::new("assetiweave-adapter-runtime-version-fixture");
    let adapter_path = fixture.path().join("adapter.mjs");
    fs::write(&adapter_path, "#!/usr/bin/env node\n").unwrap();
    let manifest_path = fixture.path().join("conversation-adapter.json");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "id": "fixture-runtime-version",
            "name": "Fixture Runtime Version",
            "version": "0.1.0",
            "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
            "runtime": {
                "type": "node",
                "entry": "adapter.mjs",
                "version": "^20"
            },
            "capabilities": ["probe", "read_session"],
            "input_kinds": ["directory"]
        }))
        .unwrap(),
    )
    .unwrap();

    let error = validate_external_adapter_manifest(manifest_path.to_string_lossy().as_ref())
        .expect_err("unsupported runtime version constraint should fail validation");

    assert!(error.contains("runtime version constraint"));
    assert!(error.contains(">=x"));
}

#[test]
fn external_adapter_validation_rejects_runtime_entry_outside_adapter_directory() {
    for entry in [
        "../adapter.mjs",
        r"..\adapter.mjs",
        "/tmp/adapter.mjs",
        r"C:\tmp\adapter.mjs",
    ] {
        let fixture = TempFixture::new("assetiweave-adapter-runtime-escape-fixture");
        let manifest_path = fixture.path().join("conversation-adapter.json");
        fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "id": "fixture-runtime-escape",
                "name": "Fixture Runtime Escape",
                "version": "0.1.0",
                "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
                "runtime": {
                    "type": "node",
                    "entry": entry,
                    "version": ">=20"
                },
                "capabilities": ["probe", "read_session"],
                "input_kinds": ["directory"]
            }))
            .unwrap(),
        )
        .unwrap();

        let error = validate_external_adapter_manifest(manifest_path.to_string_lossy().as_ref())
            .expect_err("unsafe runtime entry should fail validation");

        assert!(
            error.contains("adapter runtime entry"),
            "entry {entry:?} produced error {error:?}"
        );
    }
}

#[test]
fn external_adapter_validation_rejects_legacy_command_outside_adapter_directory() {
    let fixture = TempFixture::new("assetiweave-adapter-command-escape-fixture");
    let manifest_path = fixture.path().join("conversation-adapter.json");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "id": "fixture-command-escape",
            "name": "Fixture Command Escape",
            "version": "0.1.0",
            "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
            "command": ["../adapter.sh"],
            "capabilities": ["probe", "read_session"],
            "input_kinds": ["directory"]
        }))
        .unwrap(),
    )
    .unwrap();

    let error = validate_external_adapter_manifest(manifest_path.to_string_lossy().as_ref())
        .expect_err("unsafe legacy command should fail validation");

    assert!(error.contains("adapter command"));
    assert!(error.contains("escape"));
}

#[test]
fn external_adapter_scaffold_generates_export_markdown_fixtures() {
    let fixture = TempFixture::new("assetiweave-adapter-scaffold-fixture");

    let result = scaffold_external_adapter(ExternalAdapterScaffoldParams {
        directory: fixture.path().to_string_lossy().to_string(),
        id: "fixture-external".to_string(),
        name: "Fixture External".to_string(),
        dry_run: false,
    })
    .unwrap();

    let manifest: ConversationAdapterManifest =
        serde_json::from_str(&fs::read_to_string(&result.manifest_path).unwrap()).unwrap();
    assert_eq!(
        manifest
            .runtime
            .as_ref()
            .map(|runtime| runtime.entry.as_str()),
        Some("adapter-executable")
    );
    validate_external_adapter_manifest(&result.manifest_path).unwrap();
    assert!(manifest
        .capabilities
        .contains(&"export_markdown".to_string()));

    let request: Value =
        serde_json::from_str(&fs::read_to_string(&result.export_request_fixture_path).unwrap())
            .unwrap();
    assert_eq!(request["method"], "export_markdown");
    assert_eq!(
        request["params"]["default_relative_path"],
        "example/Example-session.md"
    );
    assert!(request["params"]["session_detail"].is_object());

    let response = fs::read(&result.export_response_fixture_path).unwrap();
    let parsed = parse_external_adapter_output("export_markdown", response, Vec::new()).unwrap();
    let export = parsed.markdown_export.expect("markdown export fixture");
    assert_eq!(export.relative_path, "example/Example-session.md");
    assert!(export.content.contains("## 1. Example question"));
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
fn external_adapter_try_run_parses_markdown_export() {
    let fixture = TempFixture::new("assetiweave-adapter-export-run-fixture");
    write_executable_script(
        fixture.path(),
        "adapter.sh",
        r##"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"item","item":{"kind":"markdown_export","content":"# Exported from adapter","relative_path":"fixture/export.md"}}'
printf '%s\n' '{"type":"complete","item":{"export_count":1}}'
"##,
    );
    let manifest = write_manifest(fixture.path(), vec!["adapter.sh".to_string()]);

    let result = try_run_external_adapter(ExternalAdapterTryRunParams {
        manifest_path: manifest.to_string_lossy().to_string(),
        method: "export_markdown".to_string(),
        location: Some(fixture.path().to_string_lossy().to_string()),
        session_id: None,
        yes: true,
    })
    .unwrap();

    let export = result.markdown_export.expect("markdown export");
    assert_eq!(result.item_count, 1);
    assert_eq!(export.content, "# Exported from adapter");
    assert_eq!(export.relative_path, "fixture/export.md");
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
fn official_codex_adapter_splits_command_and_result_cards() {
    if !command_available("node") || !command_available("sqlite3") {
        return;
    }
    let fixture = TempFixture::new("assetiweave-official-codex-fixture");
    let rollout = fixture.path().join("rollout.jsonl");
    fs::write(
        &rollout,
        [
            r#"{"payload":{"type":"message","role":"user","id":"turn-context","content":"Repository context only"}}"#,
            r#"{"payload":{"type":"message","role":"user","id":"turn-1","content":"Run tests"}}"#,
            r#"{"payload":{"type":"message","role":"assistant","content":"Use this:\n```sh\ncargo test\n```"}}"#,
            r#"{"payload":{"type":"function_call","name":"update_plan","arguments":"{\"plan\":[]}"}}"#,
            r#"{"payload":{"type":"exec","command":"cargo test","output":"tests passed","status":"completed","exit_code":0}}"#,
        ]
        .join("\n"),
    )
    .unwrap();
    let db_path = fixture.path().join("state_5.sqlite");
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT, title TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO threads (id, rollout_path, title) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "codex-session-1",
                rollout.to_string_lossy().to_string(),
                "Codex fixture"
            ],
        )
        .unwrap();
    }
    let adapter = official_adapter_fixture(
        "codex",
        "Codex",
        "bundled/conversation-adapters/codex/conversation-adapter.json",
        vec![ConversationSourceKind::Live, ConversationSourceKind::File],
    );
    let source = source_fixture(
        "codex",
        ConversationSourceKind::Live,
        &fixture.path().to_string_lossy(),
    );

    let sessions = read_source_sessions_with_adapter(Some(&adapter), &source).unwrap();

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].turns.len(), 1);
    assert_eq!(sessions[0].turns[0].turn_index, 0);
    assert_eq!(sessions[0].turns[0].user_text, "Run tests");
    let parts = &sessions[0].turns[0].parts;
    let card_types = parts
        .iter()
        .filter_map(|part| content_card_type(part.metadata_json.as_deref()))
        .collect::<Vec<_>>();
    assert_eq!(
        card_types,
        vec![
            "answer".to_string(),
            "code".to_string(),
            "tool".to_string(),
            "command".to_string(),
            "result".to_string()
        ]
    );
    assert_eq!(parts[2].text.as_deref(), Some("function_call: update_plan"));
    assert_eq!(parts[3].command.as_deref(), Some("cargo test"));
    assert_eq!(parts[4].text.as_deref(), Some("tests passed"));
}

#[cfg(unix)]
#[test]
fn official_adapters_export_markdown_from_standard_session_detail() {
    if !command_available("node") {
        return;
    }
    for manifest_relative_path in [
        "bundled/conversation-adapters/codex/conversation-adapter.json",
        "bundled/conversation-adapters/opencode/conversation-adapter.json",
        "bundled/conversation-adapters/claude-code/conversation-adapter.json",
    ] {
        let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(manifest_relative_path);
        let validation =
            validate_external_adapter_manifest(manifest_path.to_string_lossy().as_ref()).unwrap();
        let request = json!({
            "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
            "request_id": "fixture-export-markdown",
            "method": "export_markdown",
            "source": { "location": ".", "config": null },
            "params": {
                "session_detail": export_session_detail_fixture(),
                "question_ids": ["question-1"],
                "content_filter": {
                    "answer": true,
                    "tool": true,
                    "command": false,
                    "code": true,
                    "result": true
                },
                "record_kind": "session",
                "default_relative_path": "adapter/project/custom.md"
            }
        });

        let result = run_external_adapter(
            &validation,
            "export_markdown",
            request,
            Duration::from_millis(DEFAULT_READ_TIMEOUT_MS),
        )
        .unwrap();
        let export = result.markdown_export.expect("markdown export");

        assert_eq!(export.relative_path, "adapter/project/custom.md");
        assert!(export.content.contains("## 1. Export this"));
        assert!(!export.content.contains("## Session Metadata"));
        assert!(export
            .content
            .contains("### Answer\n\n```markdown\n# visible answer\n\n## nested heading\n```"));
        assert!(export
            .content
            .contains("### Code\n\n```ts\nconst ok = true;\n```"));
        assert!(export
            .content
            .contains("### Result\n\n```\n# result heading\n```"));
        assert!(!export.content.contains("raw hidden answer"));
        assert!(!export.content.contains("pnpm test"));
    }
}

#[cfg(unix)]
#[test]
fn official_codex_adapter_does_not_embed_raw_tool_payload_metadata() {
    if !command_available("node") || !command_available("sqlite3") {
        return;
    }
    let fixture = TempFixture::new("assetiweave-official-codex-large-fixture");
    let rollout = fixture.path().join("rollout.jsonl");
    let large_payload = format!("hidden-codex-payload-{}", "x".repeat(32 * 1024));
    fs::write(
        &rollout,
        [
            r#"{"payload":{"type":"message","role":"user","id":"turn-1","content":"Run tests"}}"#
                .to_string(),
            r#"{"payload":{"type":"exec","command":"cargo test","output":"tests passed","status":"completed","exit_code":0,"debug":{"blob":""#.to_string()
                + &large_payload
                + r#""}}}"#,
        ]
        .join("\n"),
    )
    .unwrap();
    let db_path = fixture.path().join("state_5.sqlite");
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT, title TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO threads (id, rollout_path, title) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "codex-session-1",
                rollout.to_string_lossy().to_string(),
                "Codex large fixture"
            ],
        )
        .unwrap();
    }
    let adapter = official_adapter_fixture(
        "codex",
        "Codex",
        "bundled/conversation-adapters/codex/conversation-adapter.json",
        vec![ConversationSourceKind::Live, ConversationSourceKind::File],
    );
    let source = source_fixture(
        "codex",
        ConversationSourceKind::Live,
        &fixture.path().to_string_lossy(),
    );

    let sessions = read_source_sessions_with_adapter(Some(&adapter), &source).unwrap();

    assert_eq!(sessions.len(), 1);
    let parts = &sessions[0].turns[0].parts;
    assert_content_card_types(parts, &["command", "result"]);
    assert_eq!(parts[0].command.as_deref(), Some("cargo test"));
    assert_eq!(parts[1].text.as_deref(), Some("tests passed"));
    let metadata = parts
        .iter()
        .filter_map(|part| part.metadata_json.as_deref())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!metadata.contains("hidden-codex-payload"));
}

#[cfg(unix)]
#[test]
fn official_codex_adapter_truncates_large_browse_text() {
    if !command_available("node") || !command_available("sqlite3") {
        return;
    }
    let fixture = TempFixture::new("assetiweave-official-codex-truncate-fixture");
    let rollout = fixture.path().join("rollout.jsonl");
    let large_output = format!("large-output-start\n{}", "z".repeat(512 * 1024));
    fs::write(
        &rollout,
        [
            r#"{"payload":{"type":"message","role":"user","id":"turn-1","content":"Run tests"}}"#
                .to_string(),
            json!({
                "payload": {
                    "type": "exec",
                    "command": "cargo test",
                    "output": large_output,
                    "status": "completed",
                    "exit_code": 0
                }
            })
            .to_string(),
        ]
        .join("\n"),
    )
    .unwrap();
    let db_path = fixture.path().join("state_5.sqlite");
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT, title TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO threads (id, rollout_path, title) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "codex-session-1",
                rollout.to_string_lossy().to_string(),
                "Codex truncate fixture"
            ],
        )
        .unwrap();
    }
    let adapter = official_adapter_fixture(
        "codex",
        "Codex",
        "bundled/conversation-adapters/codex/conversation-adapter.json",
        vec![ConversationSourceKind::Live, ConversationSourceKind::File],
    );
    let source = source_fixture(
        "codex",
        ConversationSourceKind::Live,
        &fixture.path().to_string_lossy(),
    );

    let sessions = read_source_sessions_with_adapter(Some(&adapter), &source).unwrap();

    let result_part = &sessions[0].turns[0].parts[1];
    assert_eq!(result_part.command.as_deref(), None);
    assert!(result_part
        .text
        .as_deref()
        .unwrap()
        .contains("large-output-start"));
    assert!(result_part.text.as_deref().unwrap().len() < 128 * 1024);
    let metadata: Value = serde_json::from_str(result_part.metadata_json.as_deref().unwrap())
        .expect("result metadata");
    assert_eq!(
        metadata.get("truncated").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        metadata
            .get("content_card")
            .and_then(|value| value.get("type"))
            .and_then(Value::as_str),
        Some("result")
    );
}

#[cfg(unix)]
#[test]
fn official_opencode_adapter_splits_command_and_result_cards() {
    if !command_available("node") || !command_available("sqlite3") {
        return;
    }
    let fixture = TempFixture::new("assetiweave-official-opencode-fixture");
    let db_path = fixture.path().join("opencode.db");
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, project TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, role TEXT, data TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE TABLE part (message_id TEXT, session_id TEXT, kind TEXT, text TEXT, data TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, title, project) VALUES ('opencode-session-1', 'OpenCode fixture', '/tmp/project')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message (id, session_id, role, data) VALUES ('m0', 'opencode-session-1', 'user', NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO part (message_id, session_id, kind, text, data) VALUES ('m0', 'opencode-session-1', 'text', 'Repository context only', NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message (id, session_id, role, data) VALUES ('m1', 'opencode-session-1', 'user', NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO part (message_id, session_id, kind, text, data) VALUES ('m1', 'opencode-session-1', 'text', 'Run tests', NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message (id, session_id, role, data) VALUES ('m2', 'opencode-session-1', 'assistant', NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO part (message_id, session_id, kind, text, data) VALUES ('m2', 'opencode-session-1', 'text', 'Use this:\n```sh\ncargo test\n```', NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message (id, session_id, role, data) VALUES ('m3', 'opencode-session-1', 'assistant', NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO part (message_id, session_id, kind, text, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                "m3",
                "opencode-session-1",
                "command",
                "tests passed",
                r#"{"command":"cargo test","status":"completed","exit_code":0}"#,
            ],
        )
        .unwrap();
    }
    let adapter = official_adapter_fixture(
        "opencode",
        "OpenCode",
        "bundled/conversation-adapters/opencode/conversation-adapter.json",
        vec![ConversationSourceKind::Live, ConversationSourceKind::Sqlite],
    );
    let source = source_fixture(
        "opencode",
        ConversationSourceKind::Sqlite,
        &db_path.to_string_lossy(),
    );

    let sessions = read_source_sessions_with_adapter(Some(&adapter), &source).unwrap();

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].turns.len(), 1);
    assert_eq!(sessions[0].turns[0].turn_index, 0);
    assert_eq!(sessions[0].turns[0].user_text, "Run tests");
    assert_content_card_types(
        &sessions[0].turns[0].parts,
        &["answer", "code", "command", "result"],
    );
    assert_eq!(
        sessions[0].turns[0].parts[2].command.as_deref(),
        Some("cargo test")
    );
    assert_eq!(
        sessions[0].turns[0].parts[3].text.as_deref(),
        Some("tests passed")
    );
}

#[cfg(unix)]
#[test]
fn official_opencode_adapter_extracts_json_fields_without_raw_metadata() {
    if !command_available("node") || !command_available("sqlite3") {
        return;
    }
    let fixture = TempFixture::new("assetiweave-official-opencode-json-fixture");
    let db_path = fixture.path().join("opencode.db");
    let large_diff = format!("hidden-large-diff-{}", "x".repeat(32 * 1024));
    let large_attachment = format!("hidden-large-attachment-{}", "y".repeat(32 * 1024));
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, directory TEXT, time_updated INTEGER)",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE TABLE part (message_id TEXT, session_id TEXT, data TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, title, directory, time_updated) VALUES ('opencode-session-1', 'OpenCode JSON fixture', '/tmp/project', 4)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                "m1",
                "opencode-session-1",
                1,
                json!({
                    "role": "user",
                    "time": { "created": 1 },
                    "summary": { "diffs": [{ "before": large_diff, "after": "small" }] }
                })
                .to_string()
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO part (message_id, session_id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "m1",
                "opencode-session-1",
                json!({ "type": "text", "text": "Run tests" }).to_string()
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                "m2",
                "opencode-session-1",
                2,
                json!({ "role": "assistant", "time": { "created": 2 } }).to_string()
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO part (message_id, session_id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "m2",
                "opencode-session-1",
                json!({ "type": "text", "text": "Use this:\n```sh\ncargo test\n```" }).to_string()
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                "m3",
                "opencode-session-1",
                3,
                json!({ "role": "assistant", "time": { "created": 3 } }).to_string()
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO part (message_id, session_id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "m3",
                "opencode-session-1",
                json!({
                    "type": "tool",
                    "tool": "bash",
                    "state": {
                        "status": "completed",
                        "input": {
                            "command": "cargo test",
                            "cwd": "/tmp/project",
                            "description": "Run Rust tests"
                        },
                        "output": "tests passed",
                        "metadata": {
                            "output": "tests passed",
                            "exit": 0,
                            "description": "Run Rust tests"
                        },
                        "title": "Run Rust tests"
                    },
                    "attachments": [{ "url": large_attachment }]
                })
                .to_string()
            ],
        )
        .unwrap();
    }
    let adapter = official_adapter_fixture(
        "opencode",
        "OpenCode",
        "bundled/conversation-adapters/opencode/conversation-adapter.json",
        vec![ConversationSourceKind::Live, ConversationSourceKind::Sqlite],
    );
    let source = source_fixture(
        "opencode",
        ConversationSourceKind::Sqlite,
        &db_path.to_string_lossy(),
    );

    let sessions = read_source_sessions_with_adapter(Some(&adapter), &source).unwrap();

    assert_eq!(sessions.len(), 1);
    let parts = &sessions[0].turns[0].parts;
    assert_content_card_types(parts, &["answer", "code", "command", "result"]);
    assert_eq!(parts[2].command.as_deref(), Some("cargo test"));
    assert_eq!(parts[3].text.as_deref(), Some("tests passed"));
    let metadata = parts
        .iter()
        .filter_map(|part| part.metadata_json.as_deref())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!metadata.contains("hidden-large-attachment"));
    assert!(!metadata.contains("hidden-large-diff"));
}

#[cfg(unix)]
#[test]
fn official_claude_code_adapter_splits_command_and_result_cards() {
    if !command_available("node") {
        return;
    }
    let fixture = TempFixture::new("assetiweave-official-claude-fixture");
    let jsonl = fixture.path().join("session.jsonl");
    fs::write(
        &jsonl,
        [
            r#"{"type":"message","role":"user","uuid":"turn-context","content":"Repository context only"}"#,
            r#"{"type":"message","role":"user","uuid":"turn-1","content":"Run tests"}"#,
            r#"{"type":"message","role":"assistant","content":"Use this:\n```sh\ncargo test\n```"}"#,
            r#"{"type":"shell","command":"cargo test","output":"tests passed","status":"completed","exit_code":0}"#,
        ]
        .join("\n"),
    )
    .unwrap();
    let adapter = official_adapter_fixture(
        "claude-code",
        "Claude Code",
        "bundled/conversation-adapters/claude-code/conversation-adapter.json",
        vec![
            ConversationSourceKind::Live,
            ConversationSourceKind::Directory,
            ConversationSourceKind::File,
        ],
    );
    let source = source_fixture(
        "claude-code",
        ConversationSourceKind::Directory,
        &fixture.path().to_string_lossy(),
    );

    let sessions = read_source_sessions_with_adapter(Some(&adapter), &source).unwrap();

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].turns.len(), 1);
    assert_eq!(sessions[0].turns[0].turn_index, 0);
    assert_eq!(sessions[0].turns[0].user_text, "Run tests");
    assert_content_card_types(
        &sessions[0].turns[0].parts,
        &["answer", "code", "command", "result"],
    );
    assert_eq!(
        sessions[0].turns[0].parts[2].command.as_deref(),
        Some("cargo test")
    );
    assert_eq!(
        sessions[0].turns[0].parts[3].text.as_deref(),
        Some("tests passed")
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

#[cfg(unix)]
fn official_adapter_fixture(
    id: &str,
    name: &str,
    manifest_relative_path: &str,
    input_kinds: Vec<ConversationSourceKind>,
) -> ConversationAdapter {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(manifest_relative_path);
    ConversationAdapter {
        id: id.to_string(),
        name: name.to_string(),
        kind: ConversationAdapterKind::External,
        version: "1.0.0".to_string(),
        enabled: true,
        manifest_path: Some(manifest_path.to_string_lossy().to_string()),
        executable_path: None,
        content_hash: None,
        trusted_hash: None,
        trust_state: ConversationAdapterTrustState::BuiltIn,
        protocol_version: Some(EXTERNAL_ADAPTER_PROTOCOL_VERSION),
        capabilities: vec![
            "probe".to_string(),
            "list_sessions".to_string(),
            "read_session".to_string(),
            "export_markdown".to_string(),
        ],
        input_kinds,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

#[cfg(unix)]
fn content_card_type(metadata_json: Option<&str>) -> Option<String> {
    let value: Value = serde_json::from_str(metadata_json?).ok()?;
    value
        .get("content_card")?
        .get("type")?
        .as_str()
        .map(ToString::to_string)
}

#[cfg(unix)]
fn assert_content_card_types(
    parts: &[crate::backend::models::NormalizedConversationPart],
    expected: &[&str],
) {
    let card_types = parts
        .iter()
        .filter_map(|part| content_card_type(part.metadata_json.as_deref()))
        .collect::<Vec<_>>();
    assert_eq!(
        card_types,
        expected
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
    );
}

fn export_session_detail_fixture() -> serde_json::Value {
    json!({
        "session": {
            "id": "session-1",
            "source_id": "source-1",
            "adapter_id": "adapter-1",
            "external_id": "external-session-1",
            "title": "Export Fixture",
            "project_path": "/tmp/project",
            "started_at": null,
            "updated_at": null,
            "source_locator": null,
            "source_fingerprint": null,
            "missing": false,
            "created_at": "2026-01-01T00:00:00Z",
            "imported_at": "2026-01-01T00:00:00Z"
        },
        "questions": [{
            "question": {
                "id": "question-1",
                "session_id": "session-1",
                "question_index": 0,
                "title": null,
                "question_text": "Export this",
                "answer_text": "visible answer",
                "code_text": "const ok = true;",
                "command_text": "pnpm test",
                "grouping_origin": "imported",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z"
            },
            "turns": [{
                "id": "turn-1",
                "session_id": "session-1",
                "external_id": "turn-1",
                "turn_index": 0,
                "user_text": "Export this",
                "title": null,
                "started_at": null,
                "ended_at": null,
                "fingerprint": "turn-fingerprint",
                "missing": false,
                "imported_at": "2026-01-01T00:00:00Z"
            }],
            "parts": [
                {
                    "id": "part-answer",
                    "turn_id": "turn-1",
                    "part_index": 0,
                    "role": "assistant",
                    "kind": "text",
                    "text": "raw hidden answer",
                    "language": null,
                    "command": null,
                    "cwd": null,
                    "status": null,
                    "exit_code": null,
                    "metadata_json": "{\"content_card\":{\"type\":\"answer\",\"format\":\"markdown\",\"text\":\"# visible answer\\n\\n## nested heading\"}}"
                },
                {
                    "id": "part-code",
                    "turn_id": "turn-1",
                    "part_index": 1,
                    "role": "assistant",
                    "kind": "code_block",
                    "text": "const ok = true;",
                    "language": "ts",
                    "command": null,
                    "cwd": null,
                    "status": null,
                    "exit_code": null,
                    "metadata_json": "{\"content_card\":{\"type\":\"code\",\"language\":\"ts\"}}"
                },
                {
                    "id": "part-command",
                    "turn_id": "turn-1",
                    "part_index": 2,
                    "role": "tool",
                    "kind": "command",
                    "text": null,
                    "language": null,
                    "command": "pnpm test",
                    "cwd": "/tmp/project",
                    "status": "completed",
                    "exit_code": 0,
                    "metadata_json": "{\"content_card\":{\"type\":\"command\"}}"
                },
                {
                    "id": "part-result",
                    "turn_id": "turn-1",
                    "part_index": 3,
                    "role": "tool",
                    "kind": "tool",
                    "text": "# result heading",
                    "language": null,
                    "command": null,
                    "cwd": null,
                    "status": "completed",
                    "exit_code": 0,
                    "metadata_json": "{\"content_card\":{\"type\":\"result\",\"format\":\"plain\"}}"
                }
            ]
        }]
    })
}

#[cfg(unix)]
fn command_available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
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
        runtime: None,
        capabilities: vec![
            "probe".to_string(),
            "list_sessions".to_string(),
            "read_session".to_string(),
            "export_markdown".to_string(),
        ],
        input_kinds: vec![ConversationSourceKind::Directory],
    };
    let path = dir.join("conversation-adapter.json");
    fs::write(&path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
    path
}
