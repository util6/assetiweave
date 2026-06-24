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
