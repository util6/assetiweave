// Adversarial review tests for Spec #117 / Tickets #118-#123 (Round 1 & Round 2)
// Enforces invariants against subtle bugs, silent drops, and data poisoning.

// Adversarial review test suite for Spec #117 (commits bf96eacb..72ac1c55)
// Designed to empirically verify subtle bugs missed by the happy-path ticket tests.

use crate::backend::application::{
    AppService, ConversationBlockGetParams, ConversationDataAuditParams, ConversationOutlineParams,
};
use crate::backend::domain::{
    ConversationCardRenderer, ConversationContentCardDescriptor, ConversationPartKind,
    ConversationPartRole, ConversationRecordKind, ConversationSource, ConversationSourceKind,
    NormalizedConversationPart, NormalizedConversationSession, NormalizedConversationTurn,
};
use crate::backend::store::{
    import_conversation_sessions_sqlx, import_web_record_sessions_sqlx,
    list_conversation_sessions_by_id_fragment_sqlx, list_conversation_sessions_sqlx,
    list_web_record_sessions_sqlx, load_conversation_block_detail_sqlx,
    load_conversation_session_detail_sqlx, load_web_record_session_detail_sqlx, open_migrated_pool,
    resolve_any_id_to_session_id_sqlx, resolve_conversation_part_id_prefix_sqlx,
    resolve_outline_target_sqlx, resolve_web_record_part_id_prefix_sqlx, StoreError,
};
use serde_json::json;
use std::path::PathBuf;
use uuid::Uuid;

struct TempTestDir(PathBuf);

impl TempTestDir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("aiw-adv-review-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn db_path(&self) -> PathBuf {
        self.0.join("app.db")
    }
}

impl Drop for TempTestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn setup_pool() -> (TempTestDir, sqlx::SqlitePool) {
    let dir = TempTestDir::new();
    let pool = open_migrated_pool(&dir.db_path()).await.unwrap();
    (dir, pool)
}

async fn seed_source_and_adapter(
    pool: &sqlx::SqlitePool,
    adapter_id: &str,
    source_id: &str,
    card_kinds_json: &str,
) -> ConversationSource {
    let now = "2026-10-24T12:00:00Z";
    sqlx::query(
        r#"
        INSERT OR REPLACE INTO conversation_adapters (
            tenant_id, id, name, kind, version, enabled, trust_state,
            capabilities, input_kinds, card_contract_version, card_kinds_json,
            created_at, updated_at
        ) VALUES ('default', ?1, ?1, 'external', '1.0.0', 1, 'built_in', '[]', '["directory"]', 1, ?2, ?3, ?3)
        "#,
    )
    .bind(adapter_id)
    .bind(card_kinds_json)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();

    let source = ConversationSource {
        id: source_id.to_string(),
        adapter_id: adapter_id.to_string(),
        name: source_id.to_string(),
        kind: ConversationSourceKind::Directory,
        location: "/tmp".to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: now.to_string(),
        updated_at: now.to_string(),
    };
    sqlx::query(
        r#"
        INSERT OR REPLACE INTO conversation_sources (
            tenant_id, id, adapter_id, name, kind, location, enabled, created_at, updated_at
        ) VALUES ('default', ?1, ?2, ?1, 'directory', '/tmp', 1, ?3, ?3)
        "#,
    )
    .bind(source_id)
    .bind(adapter_id)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    source
}

// =========================================================================
// ROUND 1 ADVERSARIAL TESTS (Bugs 1-7)
// =========================================================================

/// BUG 1: Web record answers lose Markdown renderer because read SQL sets `NULL AS attrs_json`,
/// causing `descriptor.renderer` to be `None` -> `ConversationCardRenderer::Plain`.
#[tokio::test]
async fn bug1_web_record_answer_loses_markdown_renderer() {
    let (_dir, pool) = setup_pool().await;
    let card_kinds = r#"[
        {"id":"chatgpt-web.answer","semantic_role":"answer","label":"Answer","default_renderer":"markdown","allowed_renderers":["markdown","plain"]},
        {"id":"chatgpt-web.result","semantic_role":"result","label":"Result","default_renderer":"plain","allowed_renderers":["plain","markdown"]}
    ]"#;
    let source = seed_source_and_adapter(&pool, "chatgpt-web", "chatgpt-src", card_kinds).await;

    let session = NormalizedConversationSession {
        external_id: "web-1".to_string(),
        title: Some("Web Markdown Test".to_string()),
        turns: vec![NormalizedConversationTurn {
            external_id: "t-1".to_string(),
            turn_index: 0,
            user_text: "Show rust code".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            model: None,
            parts: vec![
                NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Text,
                    text: Some("# Title\n\n- item".to_string()),
                    language: None,
                    command: None,
                    cwd: None,
                    status: None,
                    exit_code: None,
                    command_label: None,
                    source_execution_id: None,
                    content_card: Some(ConversationContentCardDescriptor {
                        schema_version: 1,
                        kind: "chatgpt-web.answer".to_string(),
                        semantic_role: Some("answer".to_string()),
                        renderer: Some("markdown".to_string()),
                    }),
                    metadata_json: None,
                },
                NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::CodeBlock,
                    text: Some("fn main() {}".to_string()),
                    language: Some("rust".to_string()),
                    command: None,
                    cwd: None,
                    status: None,
                    exit_code: None,
                    command_label: None,
                    source_execution_id: None,
                    content_card: None,
                    metadata_json: None,
                },
            ],
        }],
        ..Default::default()
    };

    import_web_record_sessions_sqlx(&pool, "default", &source, &[session], false)
        .await
        .unwrap();
    let sessions = crate::backend::store::list_web_record_sessions_sqlx(
        &pool, "default", None, None, None, 10, 0,
    )
    .await
    .unwrap();
    let detail = load_web_record_session_detail_sqlx(&pool, "default", &sessions[0].session.id)
        .await
        .unwrap();
    let nodes = &detail.questions[0].projected_content_nodes;
    assert_eq!(nodes.len(), 2);
    assert_eq!(
        nodes[0].renderer,
        ConversationCardRenderer::Markdown,
        "Web answer node degraded to Plain renderer!"
    );
    assert_eq!(
        nodes[1].renderer,
        ConversationCardRenderer::Markdown,
        "Web fenced code_block node degraded to Plain renderer!"
    );
}

/// BUG 2: Prefix resolution cross-contaminates entity prefixes!
/// Passing `conversation-session-<hex>` or `web-record-part-<hex>` matches an Agent `conversation-part-<hex>`!
#[tokio::test]
async fn bug2_prefix_resolution_cross_entity_contamination() {
    let (_dir, pool) = setup_pool().await;
    let source = seed_source_and_adapter(
        &pool,
        "codex",
        "codex-src",
        r#"[{"id":"codex.answer","semantic_role":"answer","label":"Answer","default_renderer":"markdown","allowed_renderers":["markdown","plain"]}]"#,
    )
    .await;

    let session = NormalizedConversationSession {
        external_id: "s-1".to_string(),
        title: Some("Prefix Test".to_string()),
        turns: vec![NormalizedConversationTurn {
            external_id: "t-1".to_string(),
            turn_index: 0,
            user_text: "hello".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            model: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some("world".to_string()),
                language: None,
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
                command_label: None,
                source_execution_id: None,
                content_card: Some(ConversationContentCardDescriptor {
                    schema_version: 1,
                    kind: "codex.answer".to_string(),
                    semantic_role: Some("answer".to_string()),
                    renderer: Some("markdown".to_string()),
                }),
                metadata_json: None,
            }],
        }],
        ..Default::default()
    };
    import_conversation_sessions_sqlx(&pool, "default", &source, &[session], false)
        .await
        .unwrap();

    let list = crate::backend::store::list_conversation_sessions_sqlx(
        &pool, "default", None, None, None, 10, 0,
    )
    .await
    .unwrap();
    let detail = load_conversation_session_detail_sqlx(&pool, "default", &list[0].session.id)
        .await
        .unwrap();
    let part_id = &detail.questions[0].parts[0].id;
    let part_hex8 = &part_id["conversation-part-".len().."conversation-part-".len() + 8];

    let bogus_session_id = format!("conversation-session-{part_hex8}");
    let outline_res = resolve_outline_target_sqlx(&pool, "default", &bogus_session_id).await;
    assert!(
        matches!(outline_res, Err(StoreError::NotFound(_))),
        "Explicit conversation-session-* matched a conversation-part! Got: {outline_res:?}"
    );

    let bogus_web_part = format!("web-record-part-{part_hex8}");
    let part_res =
        resolve_conversation_part_id_prefix_sqlx(&pool, "default", &bogus_web_part).await;
    assert!(
        matches!(part_res, Err(StoreError::NotFound(_))),
        "Explicit web-record-part-* matched an Agent conversation-part! Got: {part_res:?}"
    );
}

/// BUG 3: `search_records.rs` off-by-one `id_key <= high` matches adjacent upper-bound bucket!
#[tokio::test]
async fn bug3_id_fragment_search_off_by_one_upper_bound() {
    let (_dir, pool) = setup_pool().await;
    let _source = seed_source_and_adapter(&pool, "codex", "codex-src", "[]").await;

    sqlx::query(
        r#"
        INSERT INTO conversation_sessions (
            tenant_id, id, source_id, adapter_id, external_id, title,
            missing, created_at, imported_at, execution_origin, user_visible
        ) VALUES ('default', 'conversation-session-0000000000000000000000000000000000000000000000000000000000000001', 'codex-src', 'codex', 'ext-1', 'OffByOne', 0, '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z', 'user', 1)
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO conversation_turns (
            tenant_id, id, session_id, external_id, turn_index, prompt,
            fingerprint, missing, imported_at, question_id, question_origin
        ) VALUES ('default', 'conversation-turn-0000000000000000000000000000000000000000000000000000000000000001', 'conversation-session-0000000000000000000000000000000000000000000000000000000000000001', 't-1', 0, 'hi', 'fp', 0, '2026-10-24T00:00:00Z', 'conversation-question-0000000000000000000000000000000000000000000000000000000000000001', 'imported')
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO conversation_parts (
            tenant_id, turn_id, part_index, id_key, kind, content
        ) VALUES (
            'default',
            'conversation-turn-0000000000000000000000000000000000000000000000000000000000000001',
            0,
            x'1234567900000000',
            'text',
            'adjacent bucket'
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let hits = list_conversation_sessions_by_id_fragment_sqlx(
        &pool,
        "default",
        ConversationRecordKind::Session,
        None,
        None,
        "12345678",
        10,
        0,
    )
    .await
    .unwrap();
    assert!(
        hits.is_empty(),
        "Searching for 12345678 falsely matched id_key 0x1234567900000000 due to `<=` off-by-one!"
    );
}

/// BUG 4: `get_conversation_outline` fails with `card not found in session` whenever the user passes
/// a valid unambiguous part hex prefix of length != 8 (e.g. 10 or 12 hex chars to disambiguate)!
#[tokio::test(flavor = "multi_thread")]
async fn bug4_outline_fails_on_10_or_12_char_part_prefix() {
    let (dir, pool) = setup_pool().await;
    let source = seed_source_and_adapter(
        &pool,
        "codex",
        "codex-src",
        r#"[{"id":"codex.answer","semantic_role":"answer","label":"Answer","default_renderer":"markdown","allowed_renderers":["markdown","plain"]}]"#,
    )
    .await;

    let session = NormalizedConversationSession {
        external_id: "s-outline".to_string(),
        title: Some("Outline 12-hex".to_string()),
        turns: vec![NormalizedConversationTurn {
            external_id: "t-1".to_string(),
            turn_index: 0,
            user_text: "question".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            model: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some("answer body".to_string()),
                language: None,
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
                command_label: None,
                source_execution_id: None,
                content_card: Some(ConversationContentCardDescriptor {
                    schema_version: 1,
                    kind: "codex.answer".to_string(),
                    semantic_role: Some("answer".to_string()),
                    renderer: Some("markdown".to_string()),
                }),
                metadata_json: None,
            }],
        }],
        ..Default::default()
    };
    import_conversation_sessions_sqlx(&pool, "default", &source, &[session], false)
        .await
        .unwrap();

    let list = crate::backend::store::list_conversation_sessions_sqlx(
        &pool, "default", None, None, None, 10, 0,
    )
    .await
    .unwrap();
    let detail = load_conversation_session_detail_sqlx(&pool, "default", &list[0].session.id)
        .await
        .unwrap();
    let part_id = &detail.questions[0].parts[0].id;
    let hex12 = &part_id["conversation-part-".len().."conversation-part-".len() + 12];

    let app = AppService::open_with_db_path(dir.db_path()).await.unwrap();
    let outline = app
        .get_conversation_outline(ConversationOutlineParams {
            id: hex12.to_string(),
        })
        .await;
    assert!(
        outline.is_ok(),
        "get_conversation_outline failed on 12-char unambiguous hex prefix `{hex12}`: {outline:?}"
    );
}

/// BUG 5: Command part with `command = None` and `text = Some(...)` corrupts `text -> None` on round-trip!
#[tokio::test]
async fn bug5_command_part_without_command_flips_text_to_command() {
    let (_dir, pool) = setup_pool().await;
    let source = seed_source_and_adapter(&pool, "codex", "codex-src", "[]").await;

    let session = NormalizedConversationSession {
        external_id: "s-cmd".to_string(),
        title: Some("Cmd Roundtrip".to_string()),
        turns: vec![NormalizedConversationTurn {
            external_id: "t-1".to_string(),
            turn_index: 0,
            user_text: "run".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            model: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Tool,
                kind: ConversationPartKind::Command,
                text: Some("echo hello".to_string()),
                language: None,
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
                command_label: None,
                source_execution_id: None,
                content_card: None,
                metadata_json: None,
            }],
        }],
        ..Default::default()
    };
    import_conversation_sessions_sqlx(&pool, "default", &source, &[session], false)
        .await
        .unwrap();
    let list = crate::backend::store::list_conversation_sessions_sqlx(
        &pool, "default", None, None, None, 10, 0,
    )
    .await
    .unwrap();
    let detail = load_conversation_session_detail_sqlx(&pool, "default", &list[0].session.id)
        .await
        .unwrap();
    let p = &detail.questions[0].parts[0];
    assert_eq!(
        p.text.as_deref(),
        Some("echo hello"),
        "part.text was lost and flipped into part.command ({:?})!",
        p.command
    );
}

/// BUG 6: `signal`, `tool_name`, and `file_path` in `metadata_json` are completely dropped on import!
#[tokio::test]
async fn bug6_attrs_json_signal_tool_name_file_path_wiped_on_import() {
    let (_dir, pool) = setup_pool().await;
    let source = seed_source_and_adapter(&pool, "claude-code", "cc-src", "[]").await;

    let session = NormalizedConversationSession {
        external_id: "s-attrs".to_string(),
        title: Some("Attrs Test".to_string()),
        turns: vec![NormalizedConversationTurn {
            external_id: "t-1".to_string(),
            turn_index: 0,
            user_text: "edit file".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            model: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Tool,
                kind: ConversationPartKind::Text,
                text: Some("Updated src/main.rs".to_string()),
                language: None,
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
                command_label: None,
                source_execution_id: None,
                content_card: None,
                metadata_json: Some(
                    r#"{"signal":"low","tool_name":"Edit","file_path":"/workspace/src/main.rs"}"#
                        .to_string(),
                ),
            }],
        }],
        ..Default::default()
    };
    import_conversation_sessions_sqlx(&pool, "default", &source, &[session], false)
        .await
        .unwrap();

    let raw_attrs: Option<String> =
        sqlx::query_scalar("SELECT attrs_json FROM conversation_parts LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let parsed: serde_json::Value = raw_attrs
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(json!({}));
    assert_eq!(
        parsed.get("tool_name").and_then(|v| v.as_str()),
        Some("Edit"),
        "tool_name was silently dropped during import! attrs_json = {raw_attrs:?}"
    );
    assert_eq!(
        parsed.get("file_path").and_then(|v| v.as_str()),
        Some("/workspace/src/main.rs"),
        "file_path was silently dropped during import! attrs_json = {raw_attrs:?}"
    );
    assert_eq!(
        parsed.get("signal").and_then(|v| v.as_str()),
        Some("low"),
        "signal was silently dropped during import! attrs_json = {raw_attrs:?}"
    );
}

/// BUG 7: Ambiguous short part IDs in `get_conversation_block` / `load_conversation_block_detail_sqlx`
/// swallow `Conflict("ambiguous ...")` into a misleading `NotFound("conversation content block not found")`!
#[tokio::test(flavor = "multi_thread")]
async fn bug7_ambiguous_short_id_swallowed_into_not_found_in_block_detail() {
    let (dir, pool) = setup_pool().await;
    let _source = seed_source_and_adapter(&pool, "codex", "codex-src", "[]").await;

    sqlx::query(
        r#"
        INSERT INTO conversation_sessions (
            tenant_id, id, source_id, adapter_id, external_id, title,
            missing, created_at, imported_at, execution_origin, user_visible
        ) VALUES ('default', 'conversation-session-0000000000000000000000000000000000000000000000000000000000000002', 'codex-src', 'codex', 'ext-2', 'Ambiguous', 0, '2026-10-24T00:00:00Z', '2026-10-24T00:00:00Z', 'user', 1)
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO conversation_turns (
            tenant_id, id, session_id, external_id, turn_index, prompt,
            fingerprint, missing, imported_at, question_id, question_origin
        ) VALUES
        ('default', 'conversation-turn-0000000000000000000000000000000000000000000000000000000000000010', 'conversation-session-0000000000000000000000000000000000000000000000000000000000000002', 't-1', 0, 'q1', 'fp1', 0, '2026-10-24T00:00:00Z', 'conversation-question-0000000000000000000000000000000000000000000000000000000000000010', 'imported'),
        ('default', 'conversation-turn-0000000000000000000000000000000000000000000000000000000000000011', 'conversation-session-0000000000000000000000000000000000000000000000000000000000000002', 't-2', 1, 'q2', 'fp2', 0, '2026-10-24T00:00:00Z', 'conversation-question-0000000000000000000000000000000000000000000000000000000000000011', 'imported')
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO conversation_parts (tenant_id, turn_id, part_index, id_key, kind, content)
        VALUES
        ('default', 'conversation-turn-0000000000000000000000000000000000000000000000000000000000000010', 0, x'deadbeef11111111', 'text', 'part A'),
        ('default', 'conversation-turn-0000000000000000000000000000000000000000000000000000000000000011', 0, x'deadbeef22222222', 'text', 'part B')
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let store_res = load_conversation_block_detail_sqlx(
        &pool,
        "default",
        ConversationRecordKind::Session,
        "deadbeef",
    )
    .await;
    assert!(
        matches!(store_res, Err(StoreError::Conflict(_))),
        "Expected StoreError::Conflict on ambiguous short ID `deadbeef`, got: {store_res:?}"
    );

    let app = AppService::open_with_db_path(dir.db_path()).await.unwrap();
    let app_res = app
        .get_conversation_block(ConversationBlockGetParams {
            block_id: "deadbeef".to_string(),
        })
        .await;
    assert!(
        app_res
            .as_ref()
            .err()
            .is_some_and(|e| e.to_string().contains("ambiguous")),
        "Expected Conflict(ambiguous) from get_conversation_block, got: {app_res:?}"
    );
}

// =========================================================================
// ROUND 2 ADVERSARIAL TESTS (R2 Bugs 1-5)
// =========================================================================

/// R2 BUG 1: Migration `202610240005` poisons 98.8% of normal `Command` parts (those with `command = Some(...)`,
/// `text = None`, and `attrs_json != NULL`, e.g. `cwd`/`exit_code`) by setting `has_command = false`,
/// wiping `part.command -> None` on read!
#[tokio::test]
async fn r2_bug1_migration_0005_poisons_normal_command_parts() {
    let (dir, pool) = setup_pool().await;
    let source = seed_source_and_adapter(&pool, "codex", "codex-src", "[]").await;

    let session = NormalizedConversationSession {
        external_id: "s-cmd-normal".to_string(),
        title: Some("Normal Command".to_string()),
        turns: vec![NormalizedConversationTurn {
            external_id: "t-1".to_string(),
            turn_index: 0,
            user_text: "run test".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            model: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Tool,
                kind: ConversationPartKind::Command,
                text: None,
                language: None,
                command: Some("cargo test --workspace".to_string()),
                cwd: Some("/workspace".to_string()),
                status: Some("completed".to_string()),
                exit_code: Some(0),
                command_label: None,
                source_execution_id: None,
                content_card: None,
                metadata_json: None,
            }],
        }],
        ..Default::default()
    };
    import_conversation_sessions_sqlx(&pool, "default", &source, &[session], false)
        .await
        .unwrap();

    pool.close().await;

    let pool2 = open_migrated_pool(&dir.db_path()).await.unwrap();
    let list = list_conversation_sessions_sqlx(&pool2, "default", None, None, None, 10, 0)
        .await
        .unwrap();
    let detail = load_conversation_session_detail_sqlx(&pool2, "default", &list[0].session.id)
        .await
        .unwrap();
    let p = &detail.questions[0].parts[0];
    assert_eq!(
        p.command.as_deref(),
        Some("cargo test --workspace"),
        "Migration 202610240005 wiped normal part.command to None!"
    );
}

/// R2 BUG 2: Cross-entity contamination STILL exists in `resolve_any_id_to_session_id_sqlx`
/// AND for `conversation-session-<turn_hex8>` in `resolve_outline_target_sqlx`!
#[tokio::test]
async fn r2_bug2_cross_entity_contamination_still_in_any_id_and_outline_turn() {
    let (_dir, pool) = setup_pool().await;
    let source = seed_source_and_adapter(
        &pool,
        "codex",
        "codex-src",
        r#"[{"id":"codex.answer","semantic_role":"answer","label":"Answer","default_renderer":"markdown","allowed_renderers":["markdown","plain"]}]"#,
    )
    .await;

    let session = NormalizedConversationSession {
        external_id: "s-cross".to_string(),
        title: Some("Cross Entity R2".to_string()),
        turns: vec![NormalizedConversationTurn {
            external_id: "t-1".to_string(),
            turn_index: 0,
            user_text: "hello".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            model: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some("world".to_string()),
                language: None,
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
                command_label: None,
                source_execution_id: None,
                content_card: Some(ConversationContentCardDescriptor {
                    schema_version: 1,
                    kind: "codex.answer".to_string(),
                    semantic_role: Some("answer".to_string()),
                    renderer: Some("markdown".to_string()),
                }),
                metadata_json: None,
            }],
        }],
        ..Default::default()
    };
    import_conversation_sessions_sqlx(&pool, "default", &source, &[session], false)
        .await
        .unwrap();

    let list = list_conversation_sessions_sqlx(&pool, "default", None, None, None, 10, 0)
        .await
        .unwrap();
    let detail = load_conversation_session_detail_sqlx(&pool, "default", &list[0].session.id)
        .await
        .unwrap();
    let turn_id = &detail.questions[0].turns[0].id;
    let turn_hex8 = &turn_id["conversation-turn-".len().."conversation-turn-".len() + 8];
    let part_id = &detail.questions[0].parts[0].id;
    let part_hex8 = &part_id["conversation-part-".len().."conversation-part-".len() + 8];

    // 1. Explicit `conversation-session-<turn_hex8>` in outline MUST NOT match a turn!
    let bogus_session_from_turn = format!("conversation-session-{turn_hex8}");
    let outline_res = resolve_outline_target_sqlx(&pool, "default", &bogus_session_from_turn).await;
    assert!(
        matches!(outline_res, Err(StoreError::NotFound(_))),
        "Explicit conversation-session-* matched a conversation-turn! Got: {outline_res:?}"
    );

    // 2. Explicit `conversation-session-<part_hex8>` in `resolve_any_id_to_session_id_sqlx` MUST NOT match a part!
    let bogus_session_from_part = format!("conversation-session-{part_hex8}");
    let any_res =
        resolve_any_id_to_session_id_sqlx(&pool, "default", &bogus_session_from_part).await;
    assert!(
        matches!(any_res, Err(StoreError::NotFound(_))),
        "resolve_any_id_to_session_id_sqlx matched a part for explicit conversation-session-*! Got: {any_res:?}"
    );
}

/// R2 BUG 3: `resolve_web_record_part_id_prefix_sqlx` short-circuits on `len >= 36` instead of `80`,
/// returning an unresolved partial prefix when given e.g. `web-record-part-<24hex>` (40 chars)!
#[tokio::test]
async fn r2_bug3_web_part_40_char_prefix_short_circuits_unresolved() {
    let (_dir, pool) = setup_pool().await;
    let card_kinds = r#"[{"id":"chatgpt-web.answer","semantic_role":"answer","label":"Answer","default_renderer":"markdown","allowed_renderers":["markdown","plain"]}]"#;
    let source = seed_source_and_adapter(&pool, "chatgpt-web", "chatgpt-src", card_kinds).await;

    let session = NormalizedConversationSession {
        external_id: "w-prefix".to_string(),
        title: Some("Web Prefix".to_string()),
        turns: vec![NormalizedConversationTurn {
            external_id: "t-1".to_string(),
            turn_index: 0,
            user_text: "hi".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            model: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some("hello".to_string()),
                language: None,
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
                command_label: None,
                source_execution_id: None,
                content_card: None,
                metadata_json: None,
            }],
        }],
        ..Default::default()
    };
    import_web_record_sessions_sqlx(&pool, "default", &source, &[session], false)
        .await
        .unwrap();
    let sessions = list_web_record_sessions_sqlx(&pool, "default", None, None, None, 10, 0)
        .await
        .unwrap();
    let detail = load_web_record_session_detail_sqlx(&pool, "default", &sessions[0].session.id)
        .await
        .unwrap();
    let full_part_id = &detail.questions[0].parts[0].id;
    assert_eq!(full_part_id.len(), 80);

    let prefix_40 = &full_part_id[..40]; // `web-record-part-` (16) + 24 hex = 40 chars
    let resolved = resolve_web_record_part_id_prefix_sqlx(&pool, "default", prefix_40)
        .await
        .unwrap();
    assert_eq!(
        resolved, *full_part_id,
        "40-char web-record-part prefix short-circuited without resolving to full 80-char ID!"
    );
}

/// R2 BUG 4: `load_conversation_session_detail_sqlx` and `list_conversation_sessions_by_id_fragment_sqlx`
/// omit `last_cwd` in their SQL SELECT, silently returning `last_cwd = None`!
#[tokio::test]
async fn r2_bug4_session_detail_and_id_search_drop_last_cwd() {
    let (_dir, pool) = setup_pool().await;
    let source = seed_source_and_adapter(&pool, "codex", "codex-src", "[]").await;

    let session = NormalizedConversationSession {
        external_id: "s-cwd".to_string(),
        title: Some("Cwd Check".to_string()),
        turns: vec![NormalizedConversationTurn {
            external_id: "t-1".to_string(),
            turn_index: 0,
            user_text: "pwd".to_string(),
            title: None,
            started_at: Some("2026-10-24T10:00:00Z".to_string()),
            ended_at: Some("2026-10-24T10:00:05Z".to_string()),
            model: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Tool,
                kind: ConversationPartKind::Command,
                text: None,
                language: None,
                command: Some("pwd".to_string()),
                cwd: Some("/Users/demo/project".to_string()),
                status: None,
                exit_code: Some(0),
                command_label: None,
                source_execution_id: None,
                content_card: None,
                metadata_json: None,
            }],
        }],
        ..Default::default()
    };
    import_conversation_sessions_sqlx(&pool, "default", &source, &[session], false)
        .await
        .unwrap();

    let list = list_conversation_sessions_sqlx(&pool, "default", None, None, None, 10, 0)
        .await
        .unwrap();
    assert_eq!(
        list[0].session.last_cwd.as_deref(),
        Some("/Users/demo/project")
    );

    let session_id = &list[0].session.id;
    let detail = load_conversation_session_detail_sqlx(&pool, "default", session_id)
        .await
        .unwrap();
    assert_eq!(
        detail.session.last_cwd.as_deref(),
        Some("/Users/demo/project"),
        "load_conversation_session_detail_sqlx dropped last_cwd to None!"
    );

    let short8 = &session_id["conversation-session-".len().."conversation-session-".len() + 8];
    let search_hits = list_conversation_sessions_by_id_fragment_sqlx(
        &pool,
        "default",
        ConversationRecordKind::Session,
        None,
        None,
        short8,
        10,
        0,
    )
    .await
    .unwrap();
    assert_eq!(
        search_hits[0].session.last_cwd.as_deref(),
        Some("/Users/demo/project"),
        "list_conversation_sessions_by_id_fragment_sqlx dropped last_cwd to None!"
    );
}

/// R2 BUG 5: `audit_conversation_data` crashes with `sqlx::Error::RowNotFound` whenever `source_id`
/// is `Some(...)` because `web` branch uses `"SELECT 0 WHERE ?1 IS NOT NULL AND ?2 IS NULL"` with `fetch_one`!
#[tokio::test(flavor = "multi_thread")]
async fn r2_bug5_web_maintenance_audit_crashes_when_source_id_provided() {
    let (dir, pool) = setup_pool().await;
    let _source = seed_source_and_adapter(&pool, "chatgpt-web", "chatgpt-src", "[]").await;
    pool.close().await;

    let app = crate::backend::application::AppService::open_with_db_path(dir.db_path())
        .await
        .unwrap();
    let res = app
        .audit_conversation_data(crate::backend::application::ConversationDataAuditParams {
            source_id: Some("chatgpt-src".to_string()),
            ..Default::default()
        })
        .await;
    assert!(
        res.is_ok(),
        "audit_conversation_data crashed with RowNotFound when source_id was provided! Got: {res:?}"
    );
}
