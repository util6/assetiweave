#[cfg(test)]
use super::prelude::*;
use super::{
    read_source_sessions_with_adapter, try_run_external_adapter, validate_external_adapter,
};
use crate::backend::models::{ConversationPartKind, ConversationSourceKind};
use rusqlite::params;
use std::io::Cursor;

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
fn codex_adapter_reads_rollout_jsonl_from_state_database() {
    let fixture = TempFixture::new("assetiweave-codex-fixture");
    let rollout_path = fixture.path().join("rollout.jsonl");
    fs::write(
            &rollout_path,
            [
                r#"{"timestamp":"2026-01-01T00:00:00Z","type":"response_item","payload":{"id":"u1","type":"message","role":"user","content":[{"type":"input_text","text":"How should sync work?"}]}}"#,
                r#"{"timestamp":"2026-01-01T00:00:01Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Use normalized turns.\n```sh\ncargo test\n```"}]}}"#,
                r#"{"timestamp":"2026-01-01T00:00:02Z","type":"response_item","payload":{"type":"function_call","command":"cargo test","cwd":"/tmp/project","status":"completed","exit_code":0}}"#,
                r#"{"timestamp":"2026-01-01T00:00:03Z","type":"response_item","payload":{"id":"u2","type":"message","role":"user","content":"Export it"}}"#,
                r#"{"timestamp":"2026-01-01T00:00:04Z","type":"response_item","payload":{"type":"message","role":"assistant","content":"Markdown is ready."}}"#,
            ]
            .join("\n"),
        )
        .unwrap();

    let db_path = fixture.path().join("state_5.sqlite");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT, title TEXT, updated_at INTEGER);",
        )
        .unwrap();
    conn.execute(
        "INSERT INTO threads (id, rollout_path, title, updated_at) VALUES (?1, ?2, ?3, ?4)",
        params![
            "codex-session-1",
            rollout_path.to_string_lossy().as_ref(),
            "Codex Fixture",
            1_767_225_604i64
        ],
    )
    .unwrap();
    drop(conn);

    let sessions = read_source_sessions(&source_fixture(
        "codex",
        ConversationSourceKind::File,
        db_path.to_string_lossy().as_ref(),
    ))
    .unwrap();

    assert_eq!(sessions.len(), 1);
    let session = &sessions[0];
    assert_eq!(session.external_id, "codex-session-1");
    assert_eq!(session.title.as_deref(), Some("Codex Fixture"));
    assert_eq!(session.updated_at.as_deref(), Some("1767225604"));
    assert_eq!(session.project_path.as_deref(), Some("/tmp/project"));
    assert_eq!(session.turns.len(), 2);
    assert_eq!(session.turns[0].user_text, "How should sync work?");
    assert!(session.turns[0]
        .parts
        .iter()
        .any(|part| part.kind == ConversationPartKind::CodeBlock
            && part.text.as_deref() == Some("cargo test")));
    assert!(session.turns[0]
        .parts
        .iter()
        .any(|part| part.kind == ConversationPartKind::Command
            && part.command.as_deref() == Some("cargo test")));
}

#[test]
fn codex_adapter_ignores_context_only_user_messages() {
    let fixture = TempFixture::new("assetiweave-codex-context-fixture");
    let rollout_path = fixture.path().join("rollout.jsonl");
    fs::write(
            &rollout_path,
            [
                r##"{"timestamp":"2026-01-01T00:00:00Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions for /tmp/project\n\n<INSTRUCTIONS>\nUse repo rules.\n</INSTRUCTIONS>"},{"type":"input_text","text":"<environment_context>\n  <cwd>/tmp/project</cwd>\n</environment_context>"}]}}"##,
                r#"{"timestamp":"2026-01-01T00:00:01Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Please implement the conversation importer."}]}}"#,
                r#"{"timestamp":"2026-01-01T00:00:02Z","type":"response_item","payload":{"type":"message","role":"assistant","content":"Importer started."}}"#,
                r#"{"timestamp":"2026-01-01T00:00:03Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<codex_internal_context source=\"goal\">\nContinue working toward the active thread goal.\n</codex_internal_context>"}]}}"#,
                r#"{"timestamp":"2026-01-01T00:00:04Z","type":"response_item","payload":{"type":"message","role":"assistant","content":"Importer finished."}}"#,
            ]
            .join("\n"),
        )
        .unwrap();

    let db_path = fixture.path().join("state_5.sqlite");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT, title TEXT, updated_at INTEGER);",
        )
        .unwrap();
    conn.execute(
        "INSERT INTO threads (id, rollout_path, title, updated_at) VALUES (?1, ?2, ?3, ?4)",
        params![
            "codex-context-session",
            rollout_path.to_string_lossy().as_ref(),
            "Codex Context Fixture",
            1_767_225_604i64
        ],
    )
    .unwrap();
    drop(conn);

    let sessions = read_source_sessions(&source_fixture(
        "codex",
        ConversationSourceKind::File,
        db_path.to_string_lossy().as_ref(),
    ))
    .unwrap();

    assert_eq!(sessions.len(), 1);
    let turns = &sessions[0].turns;
    assert_eq!(turns.len(), 1);
    assert_eq!(
        turns[0].user_text,
        "Please implement the conversation importer."
    );
    assert!(turns[0]
        .parts
        .iter()
        .any(|part| part.text.as_deref() == Some("Importer started.")));
    assert!(turns[0]
        .parts
        .iter()
        .any(|part| part.text.as_deref() == Some("Importer finished.")));
}

#[test]
fn codex_adapter_classifies_commands_file_changes_code_blocks_and_noise() {
    let fixture = TempFixture::new("assetiweave-codex-rich-fixture");
    let rollout_path = fixture.path().join("rollout.jsonl");
    fs::write(
            &rollout_path,
            [
                json!({
                    "timestamp": "2026-01-01T00:00:00Z",
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "user",
                        "content": [{"type": "input_text", "text": "<environment_context>\n<cwd>/tmp/project</cwd>\n</environment_context>"}]
                    }
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:01Z",
                    "type": "response_item",
                    "payload": {
                        "id": "u1",
                        "type": "message",
                        "role": "user",
                        "content": [{
                            "type": "input_text",
                            "text": "Run the importer tests:\n```sh\npnpm test\n```"
                        }]
                    }
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:02Z",
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": "Use a parser.\n```rs\nlet parsed = true;\n```"}]
                    }
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:03Z",
                    "type": "response_item",
                    "payload": {
                        "type": "local_shell_call",
                        "action": {"command": "pnpm test"},
                        "cwd": "/tmp/project",
                        "status": "completed",
                        "exit_code": 0
                    }
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:04Z",
                    "type": "response_item",
                    "payload": {
                        "type": "function_call",
                        "name": "exec_command",
                        "arguments": "{\"cmd\":\"cargo test --workspace\",\"workdir\":\"/tmp/project\"}"
                    }
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:05Z",
                    "type": "response_item",
                    "payload": {
                        "type": "function_call",
                        "name": "apply_patch",
                        "arguments": "{\"patch\":\"*** Begin Patch\\n*** Update File: src/main.rs\\n@@\\n+let ok = true;\\n*** End Patch\"}"
                    }
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:06Z",
                    "type": "response_item",
                    "payload": {"type": "function_call_output", "output": "patch applied"}
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:07Z",
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "user",
                        "content": [{"type": "input_text", "text": "<codex_internal_context source=\"goal\">\nContinue active goal.\n</codex_internal_context>"}]
                    }
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:08Z",
                    "type": "response_item",
                    "payload": {
                        "id": "u2",
                        "type": "message",
                        "role": "user",
                        "content": "Export the result"
                    }
                })
                .to_string(),
            ]
            .join("\n"),
        )
        .unwrap();

    let db_path = fixture.path().join("state_5.sqlite");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT, title TEXT, updated_at INTEGER);",
        )
        .unwrap();
    conn.execute(
        "INSERT INTO threads (id, rollout_path, title, updated_at) VALUES (?1, ?2, ?3, ?4)",
        params![
            "codex-rich-session",
            rollout_path.to_string_lossy().as_ref(),
            "Codex Rich Fixture",
            1_767_225_604i64
        ],
    )
    .unwrap();
    drop(conn);

    let sessions = read_source_sessions(&source_fixture(
        "codex",
        ConversationSourceKind::File,
        db_path.to_string_lossy().as_ref(),
    ))
    .unwrap();

    let turns = &sessions[0].turns;
    assert_eq!(turns.len(), 2);
    assert!(turns[0].user_text.starts_with("Run the importer tests"));
    assert_eq!(turns[1].user_text, "Export the result");
    assert!(turns[0].parts.iter().any(|part| {
        part.kind == ConversationPartKind::CodeBlock
            && part.language.as_deref() == Some("rs")
            && part.text.as_deref() == Some("let parsed = true;")
    }));
    assert!(turns[0].parts.iter().any(|part| {
        part.kind == ConversationPartKind::Command
            && part.command.as_deref() == Some("pnpm test")
            && part.cwd.as_deref() == Some("/tmp/project")
    }));
    assert!(turns[0].parts.iter().any(|part| {
        part.kind == ConversationPartKind::Command
            && part.command.as_deref() == Some("cargo test --workspace")
            && part.cwd.as_deref() == Some("/tmp/project")
    }));
    assert!(turns[0]
        .parts
        .iter()
        .any(|part| part.kind == ConversationPartKind::FileChange));
    assert!(turns[0].parts.iter().any(|part| {
        part.kind == ConversationPartKind::Tool
            && part
                .text
                .as_deref()
                .is_some_and(|text| text.contains("patch applied"))
    }));
}

#[test]
fn claude_code_adapter_uses_real_user_boundaries_and_keeps_sidechain_output() {
    let fixture = TempFixture::new("assetiweave-claude-fixture");
    let project_dir = fixture.path().join("Users-util6-project");
    fs::create_dir_all(&project_dir).unwrap();
    let session_path = project_dir.join("claude-session.jsonl");
    fs::write(
            &session_path,
            [
                r#"{"timestamp":"2026-01-01T00:00:00Z","message":{"id":"u1","role":"user","content":"Plan the import"}}"#,
                r#"{"timestamp":"2026-01-01T00:00:01Z","message":{"role":"assistant","content":"Import as turns."}}"#,
                r#"{"timestamp":"2026-01-01T00:00:02Z","isSidechain":true,"message":{"role":"assistant","content":"Subagent checked fixture shape."}}"#,
                r#"{"timestamp":"2026-01-01T00:00:03Z","message":{"id":"u2","role":"user","content":"Now export"}}"#,
                r#"{"timestamp":"2026-01-01T00:00:04Z","message":{"role":"assistant","content":"Exported."}}"#,
            ]
            .join("\n"),
        )
        .unwrap();

    let sessions = read_source_sessions(&source_fixture(
        "claude-code",
        ConversationSourceKind::Directory,
        fixture.path().to_string_lossy().as_ref(),
    ))
    .unwrap();

    assert_eq!(sessions.len(), 1);
    let session = &sessions[0];
    assert_eq!(session.external_id, "claude-session");
    assert_eq!(session.title.as_deref(), Some("Users/util6/project"));
    assert_eq!(session.turns.len(), 2);
    assert_eq!(session.turns[0].user_text, "Plan the import");
    assert!(session.turns[0]
        .parts
        .iter()
        .any(|part| part.kind == ConversationPartKind::Subagent
            && part
                .text
                .as_deref()
                .is_some_and(|text| text.contains("Subagent checked"))));
}

#[test]
fn claude_code_adapter_uses_top_level_user_events_only_as_boundaries() {
    let fixture = TempFixture::new("assetiweave-claude-top-level-fixture");
    let session_path = fixture.path().join("claude-top-level.jsonl");
    fs::write(
            &session_path,
            [
                r#"{"timestamp":"2026-01-01T00:00:00Z","type":"user","content":"Plan the import"}"#,
                r#"{"timestamp":"2026-01-01T00:00:01Z","type":"tool_use","tool_name":"read","tool_input":{"filePath":"/tmp/project/input.jsonl"}}"#,
                r#"{"timestamp":"2026-01-01T00:00:02Z","type":"tool_result","tool_name":"read","tool_output":{"preview":"fixture rows","truncated":false}}"#,
                r#"{"timestamp":"2026-01-01T00:00:03Z","type":"user","content":"Now export it"}"#,
            ]
            .join("\n"),
        )
        .unwrap();

    let sessions = read_source_sessions(&source_fixture(
        "claude-code",
        ConversationSourceKind::File,
        session_path.to_string_lossy().as_ref(),
    ))
    .unwrap();

    assert_eq!(sessions.len(), 1);
    let turns = &sessions[0].turns;
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].user_text, "Plan the import");
    assert_eq!(turns[1].user_text, "Now export it");
    assert!(turns[0].parts.iter().any(|part| {
        part.role == ConversationPartRole::Tool
            && part
                .metadata_json
                .as_deref()
                .is_some_and(|metadata| metadata.contains("tool_use"))
    }));
}

#[test]
fn claude_code_adapter_filters_tool_result_user_wrappers_and_classifies_tools() {
    let fixture = TempFixture::new("assetiweave-claude-tool-wrapper-fixture");
    let session_path = fixture.path().join("claude-tool-wrapper.jsonl");
    fs::write(
            &session_path,
            [
                json!({
                    "timestamp": "2026-01-01T00:00:00Z",
                    "type": "user",
                    "content": "Inspect the project"
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:01Z",
                    "message": {
                        "role": "assistant",
                        "content": [{"type": "text", "text": "I will inspect it.\n```ts\nconst ok = true;\n```"}]
                    }
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:02Z",
                    "type": "user",
                    "message": {
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": "toolu_1",
                            "content": "tool result wrapper output"
                        }]
                    }
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:03Z",
                    "type": "tool_use",
                    "tool_name": "Bash",
                    "tool_input": {"command": "ls -la"}
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:04Z",
                    "type": "tool_result",
                    "tool_name": "Bash",
                    "tool_output": {"stdout": "listed files", "stderr": ""}
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:05Z",
                    "type": "result",
                    "subtype": "success",
                    "result": "bookkeeping success"
                })
                .to_string(),
                json!({
                    "timestamp": "2026-01-01T00:00:06Z",
                    "type": "user",
                    "content": [{"type": "text", "text": "Now summarize"}]
                })
                .to_string(),
            ]
            .join("\n"),
        )
        .unwrap();

    let sessions = read_source_sessions(&source_fixture(
        "claude-code",
        ConversationSourceKind::File,
        session_path.to_string_lossy().as_ref(),
    ))
    .unwrap();

    let turns = &sessions[0].turns;
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].user_text, "Inspect the project");
    assert_eq!(turns[1].user_text, "Now summarize");
    assert!(turns[0].parts.iter().any(|part| {
        part.kind == ConversationPartKind::CodeBlock
            && part.language.as_deref() == Some("ts")
            && part.text.as_deref() == Some("const ok = true;")
    }));
    assert!(turns[0].parts.iter().any(|part| {
        part.kind == ConversationPartKind::Command && part.command.as_deref() == Some("ls -la")
    }));
    assert!(turns[0].parts.iter().any(|part| {
        part.kind == ConversationPartKind::Tool
            && part
                .text
                .as_deref()
                .is_some_and(|text| text.contains("listed files"))
    }));
    assert!(turns[0].parts.iter().any(|part| {
        part.kind == ConversationPartKind::Tool
            && part
                .text
                .as_deref()
                .is_some_and(|text| text.contains("tool result wrapper output"))
    }));
    assert!(turns[0].parts.iter().all(|part| {
        !part
            .text
            .as_deref()
            .is_some_and(|text| text.contains("bookkeeping success"))
    }));
}

#[test]
fn opencode_adapter_reads_sqlite_messages_and_filters_reasoning_parts() {
    let fixture = TempFixture::new("assetiweave-opencode-fixture");
    let db_path = fixture.path().join("opencode.sqlite");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
            CREATE TABLE session (
                id TEXT PRIMARY KEY,
                title TEXT,
                project_path TEXT,
                time_updated INTEGER
            );
            CREATE TABLE message (
                id TEXT PRIMARY KEY,
                session_id TEXT,
                time_created INTEGER,
                data TEXT
            );
            CREATE TABLE part (
                session_id TEXT,
                message_id TEXT,
                data TEXT
            );
            "#,
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session (id, title, project_path, time_updated) VALUES (?1, ?2, ?3, ?4)",
        params![
            "open-session-1",
            "OpenCode Fixture",
            "/tmp/opencode",
            1_767_225_604i64
        ],
    )
    .unwrap();
    for (id, role, timestamp) in [
        ("m1", "user", 1_767_225_600i64),
        ("m2", "assistant", 1_767_225_601i64),
        ("m3", "user", 1_767_225_602i64),
        ("m4", "assistant", 1_767_225_603i64),
    ] {
        let data = json!({ "role": role, "time": timestamp }).to_string();
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            params![id, "open-session-1", timestamp, data],
        )
        .unwrap();
    }
    for (message_id, kind, text) in [
        ("m1", "text", "How should OpenCode import?"),
        ("m2", "reasoning", "hidden chain of thought"),
        (
            "m2",
            "text",
            "Use database rows.\n```ts\nconst ok = true;\n```",
        ),
        ("m2", "command", "pnpm test"),
        ("m3", "text", "Continue"),
        ("m4", "text", "Done"),
    ] {
        let data = json!({ "type": kind, "text": text }).to_string();
        conn.execute(
            "INSERT INTO part (session_id, message_id, data) VALUES (?1, ?2, ?3)",
            params!["open-session-1", message_id, data],
        )
        .unwrap();
    }
    drop(conn);

    let sessions = read_source_sessions(&source_fixture(
        "opencode",
        ConversationSourceKind::Sqlite,
        db_path.to_string_lossy().as_ref(),
    ))
    .unwrap();

    assert_eq!(sessions.len(), 1);
    let session = &sessions[0];
    assert_eq!(session.updated_at.as_deref(), Some("1767225604"));
    assert_eq!(session.turns.len(), 2);
    assert_eq!(session.turns[0].user_text, "How should OpenCode import?");
    assert!(session.turns[0]
        .parts
        .iter()
        .all(|part| part.text.as_deref() != Some("hidden chain of thought")));
    assert!(session.turns[0]
        .parts
        .iter()
        .any(|part| part.kind == ConversationPartKind::CodeBlock
            && part.language.as_deref() == Some("ts")));
    assert!(session.turns[0]
        .parts
        .iter()
        .any(|part| part.kind == ConversationPartKind::Command
            && part.text.as_deref() == Some("pnpm test")));
}

#[test]
fn opencode_adapter_ignores_user_role_bookkeeping_parts_as_turn_boundaries() {
    let fixture = TempFixture::new("assetiweave-opencode-user-bookkeeping-fixture");
    let db_path = fixture.path().join("opencode.sqlite");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
            CREATE TABLE session (
                id TEXT PRIMARY KEY,
                title TEXT,
                project_path TEXT,
                time_updated INTEGER
            );
            CREATE TABLE message (
                id TEXT PRIMARY KEY,
                session_id TEXT,
                time_created INTEGER,
                data TEXT
            );
            CREATE TABLE part (
                session_id TEXT,
                message_id TEXT,
                data TEXT
            );
            "#,
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session (id, title, project_path, time_updated) VALUES (?1, ?2, ?3, ?4)",
        params![
            "open-session-bookkeeping",
            "OpenCode Bookkeeping Fixture",
            "/tmp/opencode",
            1_767_225_604i64
        ],
    )
    .unwrap();
    for (id, role, timestamp) in [
        ("m1", "user", 1_767_225_600i64),
        ("m2", "assistant", 1_767_225_601i64),
        ("m3", "user", 1_767_225_602i64),
        ("m4", "user", 1_767_225_603i64),
        ("m5", "user", 1_767_225_604i64),
    ] {
        let data = json!({ "role": role, "time": timestamp }).to_string();
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            params![id, "open-session-bookkeeping", timestamp, data],
        )
        .unwrap();
    }
    for (message_id, kind, text) in [
        ("m1", "text", "First real question"),
        ("m2", "text", "First answer"),
        ("m3", "compaction", "Conversation compaction summary"),
        ("m4", "file", "/tmp/opencode/attachment.txt"),
        ("m5", "text", "Second real question"),
    ] {
        let data = json!({ "type": kind, "text": text }).to_string();
        conn.execute(
            "INSERT INTO part (session_id, message_id, data) VALUES (?1, ?2, ?3)",
            params!["open-session-bookkeeping", message_id, data],
        )
        .unwrap();
    }
    drop(conn);

    let sessions = read_source_sessions(&source_fixture(
        "opencode",
        ConversationSourceKind::Sqlite,
        db_path.to_string_lossy().as_ref(),
    ))
    .unwrap();

    assert_eq!(sessions.len(), 1);
    let turns = &sessions[0].turns;
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].user_text, "First real question");
    assert_eq!(turns[1].user_text, "Second real question");
}

#[test]
fn opencode_adapter_filters_synthetic_user_text_and_maps_rich_part_types() {
    let fixture = TempFixture::new("assetiweave-opencode-rich-part-fixture");
    let db_path = fixture.path().join("opencode.sqlite");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
            CREATE TABLE session (
                id TEXT PRIMARY KEY,
                title TEXT,
                project_path TEXT,
                time_updated INTEGER
            );
            CREATE TABLE message (
                id TEXT PRIMARY KEY,
                session_id TEXT,
                time_created INTEGER,
                data TEXT
            );
            CREATE TABLE part (
                session_id TEXT,
                message_id TEXT,
                data TEXT
            );
            "#,
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session (id, title, project_path, time_updated) VALUES (?1, ?2, ?3, ?4)",
        params![
            "open-session-rich",
            "OpenCode Rich Fixture",
            "/tmp/opencode",
            1_767_225_608i64
        ],
    )
    .unwrap();
    for (id, role, timestamp) in [
        ("m1", "user", 1_767_225_600i64),
        ("m2", "assistant", 1_767_225_601i64),
        ("m3", "user", 1_767_225_602i64),
        ("m4", "user", 1_767_225_603i64),
        ("m5", "user", 1_767_225_604i64),
        ("m6", "user", 1_767_225_605i64),
        ("m7", "user", 1_767_225_606i64),
    ] {
        let data = json!({ "role": role, "time": timestamp }).to_string();
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            params![id, "open-session-rich", timestamp, data],
        )
        .unwrap();
    }
    for (message_id, data) in [
        (
            "m1",
            json!({
                "type": "text",
                "text": "The following tool was executed by the user",
                "synthetic": true
            }),
        ),
        (
            "m1",
            json!({"type": "text", "text": "Real OpenCode question"}),
        ),
        (
            "m2",
            json!({"type": "reasoning", "text": "hidden reasoning"}),
        ),
        ("m2", json!({"type": "step-start", "text": "step start"})),
        ("m2", json!({"type": "retry", "text": "retry bookkeeping"})),
        (
            "m2",
            json!({"type": "snapshot", "text": "snapshot bookkeeping"}),
        ),
        (
            "m2",
            json!({
                "type": "text",
                "text": "Here is code.\n```ts\nconst rich = true;\n```"
            }),
        ),
        (
            "m2",
            json!({
                "type": "tool",
                "tool": "bash",
                "state": {
                    "status": "completed",
                    "input": {"command": "pnpm test"},
                    "output": "tests passed",
                    "title": "Run tests"
                }
            }),
        ),
        (
            "m2",
            json!({
                "type": "patch",
                "path": "src/main.ts",
                "text": "updated src/main.ts"
            }),
        ),
        (
            "m3",
            json!({"type": "text", "text": "Synthetic only", "synthetic": true}),
        ),
        (
            "m4",
            json!({"type": "text", "text": "Ignored only", "ignored": true}),
        ),
        ("m5", json!({"type": "compaction", "text": "summary only"})),
        (
            "m6",
            json!({"type": "file", "path": "/tmp/opencode/input.txt"}),
        ),
        (
            "m7",
            json!({"type": "text", "text": "Second OpenCode question"}),
        ),
    ] {
        conn.execute(
            "INSERT INTO part (session_id, message_id, data) VALUES (?1, ?2, ?3)",
            params!["open-session-rich", message_id, data.to_string()],
        )
        .unwrap();
    }
    drop(conn);

    let sessions = read_source_sessions(&source_fixture(
        "opencode",
        ConversationSourceKind::Sqlite,
        db_path.to_string_lossy().as_ref(),
    ))
    .unwrap();

    let turns = &sessions[0].turns;
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].user_text, "Real OpenCode question");
    assert_eq!(turns[1].user_text, "Second OpenCode question");
    assert!(turns[0].parts.iter().any(|part| {
        part.kind == ConversationPartKind::CodeBlock
            && part.language.as_deref() == Some("ts")
            && part.text.as_deref() == Some("const rich = true;")
    }));
    assert!(turns[0].parts.iter().any(|part| {
        part.kind == ConversationPartKind::Command
            && part.command.as_deref() == Some("pnpm test")
            && part
                .text
                .as_deref()
                .is_some_and(|text| text.contains("tests passed"))
    }));
    assert!(turns[0]
        .parts
        .iter()
        .any(|part| part.kind == ConversationPartKind::FileChange));
    assert!(turns[0].parts.iter().all(|part| {
        !part.text.as_deref().is_some_and(|text| {
            text.contains("hidden reasoning")
                || text.contains("retry bookkeeping")
                || text.contains("snapshot bookkeeping")
                || text.contains("The following tool was executed")
        })
    }));
}

#[test]
fn adapter_output_requires_complete_line() {
    let stdout = br#"{"type":"item","item":{"kind":"metadata"}}"#.to_vec();
    let error = parse_external_adapter_output("probe", stdout, Vec::new()).unwrap_err();
    assert!(error.contains("complete"));
}

#[test]
fn adapter_output_rejects_oversized_line() {
    let stdout = vec![b'a'; DEFAULT_MAX_LINE_BYTES + 1];
    let error = parse_external_adapter_output("probe", stdout, Vec::new()).unwrap_err();
    assert!(error.contains("exceeds max line size"));
}

#[test]
fn capped_reader_rejects_total_output_limit() {
    let input = vec![b'x'; 10];
    let error = read_capped(Cursor::new(input), 8).unwrap_err();
    assert!(error.contains("exceeded cap"));
}

#[cfg(unix)]
#[test]
fn external_adapter_validation_hash_changes_when_executable_changes() {
    let fixture = TempFixture::new("assetiweave-adapter-hash-fixture");
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
    write_executable_script(
        fixture.path(),
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"warning","message":"changed"}'
printf '%s\n' '{"type":"complete","item":{}}'
"#,
    );
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
printf '%s\n' '{"type":"item","item":{"kind":"session","session":{"external_id":"external-session-1","title":"External Fixture","project_path":null,"started_at":null,"updated_at":null,"source_locator":null,"source_fingerprint":null,"turns":[{"external_id":"turn-1","turn_index":0,"user_text":"External question","title":null,"started_at":null,"ended_at":null,"parts":[{"role":"assistant","kind":"text","text":"External answer","language":null,"command":null,"cwd":null,"status":null,"exit_code":null,"metadata_json":null}]}]}}}'
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
printf '%s\n' '{"type":"item","item":{"kind":"session","session":{"external_id":"web-session-1","title":"Web Fixture","project_path":null,"started_at":null,"updated_at":null,"source_locator":"fixture://web-session-1","source_fingerprint":"fixture-hash","turns":[{"external_id":"turn-1","turn_index":0,"user_text":"Web question","title":null,"started_at":null,"ended_at":null,"parts":[{"role":"assistant","kind":"text","text":"Web answer","language":null,"command":null,"cwd":null,"status":null,"exit_code":null,"metadata_json":null}]}]}}}'
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
