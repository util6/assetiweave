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
            ..Default::default()
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
            ..Default::default()
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
            fingerprint, missing, imported_at
        ) VALUES ('default', 'conversation-turn-0000000000000000000000000000000000000000000000000000000000000001', 'conversation-session-0000000000000000000000000000000000000000000000000000000000000001', 't-1', 0, 'hi', 'fp', 0, '2026-10-24T00:00:00Z')
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
            ..Default::default()
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
        .get_conversation_outline(ConversationOutlineParams::new(hex12))
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
            ..Default::default()
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
            ..Default::default()
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
            fingerprint, missing, imported_at
        ) VALUES
        ('default', 'conversation-turn-0000000000000000000000000000000000000000000000000000000000000010', 'conversation-session-0000000000000000000000000000000000000000000000000000000000000002', 't-1', 0, 'q1', 'fp1', 0, '2026-10-24T00:00:00Z'),
        ('default', 'conversation-turn-0000000000000000000000000000000000000000000000000000000000000011', 'conversation-session-0000000000000000000000000000000000000000000000000000000000000002', 't-2', 1, 'q2', 'fp2', 0, '2026-10-24T00:00:00Z')
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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

/// Adversarial stress test for M2:
/// Verify SQLite round-trip persistence of 10+ chained subagent turns with complex unicode prompts,
/// continuation turns, multiple distinct user questions, and triad parts integrity.
#[tokio::test(flavor = "multi_thread")]
async fn adversarial_stress_test_subagent_sqlite_roundtrip_persistence_and_indices() {
    let (_dir, pool) = setup_pool().await;
    let source = seed_source_and_adapter(&pool, "antigravity", "src-antigravity", "[]").await;

    // 1. Build a realistic 20-turn session
    // Q1: User turn + 12 chained subagents with complex unicode/codeblocks + 2 continuations
    // Q2: User turn + 3 subagents
    // Q3: User turn alone
    let mut turns: Vec<NormalizedConversationTurn> = Vec::new();

    // Turn 0: User turn 1
    turns.push(NormalizedConversationTurn {
        external_id: "ext-t00".to_string(),
        turn_index: 0,
        user_text: "Please analyze the concurrency bottlenecks in our database layer.".to_string(),
        title: Some("Concurrency Audit".to_string()),
        turn_kind: Some("user".to_string()),
        agent_role: None,
        parent_external_id: None,
        model: Some("gemini-2.5-pro".to_string()),
        parts: vec![NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some("Dispatching specialists to investigate database locks...".to_string()),
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
        ..Default::default()
    });

    let tricky_subagent_prompts = vec![
        "子代理任务一：请排查 SQLite 锁竞争。\n```rust\nfn check_lock() -> bool { true }\n```\n是否存在死锁风险？",
        "日本語タスク（サブエージェント2）：\n- トランザクション分離レベルを検査してください。\n- ロック時間はどのくらいですか？",
        "Subagent 3: Check WAL checkpointing.\nIs PRAGMA wal_autocheckpoint enabled?\nCheck busy_timeout: 5000ms.",
        "Subagent 4: Analyze memory pressure ⚡.\nAre connections closed promptly?",
        "Subagent 5: Evaluate connection pool size matrix:\n| Pool | Min | Max |\n|---|---|---|\n| Main | 1 | 5 |\nIs this optimal?",
        "Subagent 6: Inspect row mappers for unbounded allocations.",
        "Subagent 7: 检查索引覆盖度，特别是 `conversation_turns(session_id, turn_index)`。",
        "Subagent 8: Überprüfen Sie Thread-Sicherheit von Connection Pools.",
        "Subagent 9: Run stress benchmark script:\n```bash\ncargo test -p asset_core database_tests\n```\nReport results.",
        "Subagent 10: Hierarchical subagent verification under Subagent 9.",
        "Subagent 11: Are all mutexes dropping guards before `.await`???",
        "Subagent 12: Ingest sample JSON transaction record:\n{\n  \"status\": \"committed\"\n}\nVerify consistency.",
    ];

    for (idx, prompt) in tricky_subagent_prompts.into_iter().enumerate() {
        let turn_num = idx + 1;
        let parent_ext_id = if turn_num == 1 {
            "ext-t00".to_string()
        } else {
            format!("ext-t{:02}", turn_num - 1)
        };
        turns.push(NormalizedConversationTurn {
            external_id: format!("ext-t{turn_num:02}"),
            turn_index: turn_num as i64,
            user_text: prompt.to_string(),
            title: None,
            turn_kind: Some("subagent".to_string()),
            agent_role: Some(format!("specialist-{turn_num}")),
            parent_external_id: Some(parent_ext_id),
            model: Some("gemini-2.5-flash".to_string()),
            parts: vec![
                NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Text,
                    text: Some(format!("Specialist {turn_num} analysis underway.")),
                    language: None,
                    command: None,
                    cwd: None,
                    status: None,
                    exit_code: None,
                    command_label: None,
                    source_execution_id: None,
                    content_card: None,
                    metadata_json: None,
                },
                NormalizedConversationPart {
                    role: ConversationPartRole::Tool,
                    kind: ConversationPartKind::Command,
                    text: None,
                    language: None,
                    command: Some(format!("echo 'audit {turn_num} passed'")),
                    cwd: Some("/workspace".to_string()),
                    status: Some("success".to_string()),
                    exit_code: Some(0),
                    command_label: Some(format!("Audit step {turn_num}")),
                    source_execution_id: Some(format!("exec-{turn_num}")),
                    content_card: None,
                    metadata_json: None,
                },
                NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Text,
                    text: Some(format!("Specialist {turn_num} report: Check complete.")),
                    language: None,
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
            ..Default::default()
        });
    }

    // Turns 13 & 14: Main agent continuation turns
    turns.push(NormalizedConversationTurn {
        external_id: "ext-t13".to_string(),
        turn_index: 13,
        user_text: "Synthesizing specialist findings: Concurrency benchmarks look solid."
            .to_string(),
        title: None,
        turn_kind: Some("continuation".to_string()),
        agent_role: None,
        parent_external_id: Some("ext-t00".to_string()),
        model: Some("gemini-2.5-pro".to_string()),
        parts: vec![NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some("All 12 subagent audits confirmed zero lock contention.".to_string()),
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
        ..Default::default()
    });

    turns.push(NormalizedConversationTurn {
        external_id: "ext-t14".to_string(),
        turn_index: 14,
        user_text: "Final wrap-up for question 1: Ready for caching implementation.".to_string(),
        title: None,
        turn_kind: Some("main_continuation".to_string()),
        agent_role: None,
        parent_external_id: Some("ext-t00".to_string()),
        model: Some("gemini-2.5-pro".to_string()),
        parts: vec![NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some("Ready for next user directive.".to_string()),
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
        ..Default::default()
    });

    // Turn 15: Distinct user question 2
    turns.push(NormalizedConversationTurn {
        external_id: "ext-t15".to_string(),
        turn_index: 15,
        user_text: "Now implement the cache layer for query results.".to_string(),
        title: Some("Cache Layer Implementation".to_string()),
        turn_kind: Some("user".to_string()),
        agent_role: None,
        parent_external_id: None,
        model: Some("gemini-2.5-pro".to_string()),
        parts: vec![NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some("Delegating cache design to cache architects...".to_string()),
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
        ..Default::default()
    });

    // Turns 16..=18: 3 subagents under Question 2
    for idx in 16..=18 {
        turns.push(NormalizedConversationTurn {
            external_id: format!("ext-t{idx:02}"),
            turn_index: idx as i64,
            user_text: format!("Cache subagent {idx}: Build cache layer partition {idx}."),
            title: None,
            turn_kind: Some("subagent".to_string()),
            agent_role: Some("cache-architect".to_string()),
            parent_external_id: Some("ext-t15".to_string()),
            model: Some("gemini-2.5-flash".to_string()),
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some(format!("Cache partition {idx} active.")),
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
            ..Default::default()
        });
    }

    // Turn 19: Distinct user question 3
    turns.push(NormalizedConversationTurn {
        external_id: "ext-t19".to_string(),
        turn_index: 19,
        user_text: "Are all performance targets met?".to_string(),
        title: None,
        turn_kind: None,
        agent_role: None,
        parent_external_id: None,
        model: Some("gemini-2.5-pro".to_string()),
        parts: vec![NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some("Yes, all targets met.".to_string()),
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
        ..Default::default()
    });

    let session = NormalizedConversationSession {
        external_id: "sess-adv-stress-01".to_string(),
        title: Some("Database Concurrency & Caching Deep-Dive".to_string()),
        project_path: Some("/workspace".to_string()),
        started_at: Some("2026-10-28T00:00:00Z".to_string()),
        updated_at: Some("2026-10-28T01:00:00Z".to_string()),
        source_locator: Some("file:///workspace/session.jsonl".to_string()),
        source_fingerprint: Some("fp-stress-session-001".to_string()),
        turns,
        ..Default::default()
    };

    // 2. Import session into SQLite
    import_conversation_sessions_sqlx(&pool, "default", &source, &[session], false)
        .await
        .unwrap();

    // 3. Raw SQLite verification of columns, indices, and row counts
    let total_turns: i64 = sqlx::query_scalar("SELECT count(*) FROM conversation_turns")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total_turns, 20, "Must persist exactly 20 turns");

    let subagent_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM conversation_turns WHERE turn_kind = 'subagent'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(subagent_count, 15, "Must persist exactly 15 subagent turns");

    let continuation_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM conversation_turns WHERE turn_kind IN ('continuation', 'main_continuation')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        continuation_count, 2,
        "Must persist exactly 2 continuation turns"
    );

    // Verify parent_turn_id resolution in SQLite
    let subagent_rows: Vec<(String, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT external_id, turn_kind, agent_role, parent_turn_id FROM conversation_turns WHERE turn_kind = 'subagent' ORDER BY turn_index ASC",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

    assert_eq!(subagent_rows.len(), 15);
    for (ext_id, kind, role, parent_id) in &subagent_rows {
        assert_eq!(kind.as_deref(), Some("subagent"));
        assert!(role.is_some(), "agent_role must be persisted for {ext_id}");
        assert!(
            parent_id.is_some(),
            "parent_turn_id must be resolved for {ext_id}"
        );
    }

    // Verify index on conversation_turns exists
    let index_rows: Vec<(i64, String, i64, String, i64)> =
        sqlx::query_as("PRAGMA index_list(conversation_turns)")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(
        !index_rows.is_empty(),
        "Indices must exist on conversation_turns"
    );

    // 4. Domain session detail loading and question grouping
    let sessions =
        list_conversation_sessions_sqlx(&pool, "default", None, Some(&source.id), None, 10, 0)
            .await
            .unwrap();
    assert_eq!(sessions.len(), 1);
    let session_id = &sessions[0].session.id;

    let detail = load_conversation_session_detail_sqlx(&pool, "default", session_id)
        .await
        .unwrap();

    // Must group into exactly 3 questions
    assert_eq!(
        detail.questions.len(),
        3,
        "Must produce exactly 3 questions"
    );

    // Question 1: 15 turns
    assert_eq!(detail.questions[0].turns.len(), 15);
    assert_eq!(
        detail.questions[0].question.title.as_deref(),
        Some("Please analyze the concurrency bottlenecks in our database layer."),
        "Question 1 title must be from root user turn"
    );
    assert!(!detail.questions[0].turns[0].is_subagent());
    for t in &detail.questions[0].turns[1..=12] {
        assert!(t.is_subagent());
        assert_eq!(t.turn_kind.as_deref(), Some("subagent"));
        assert!(t.agent_role.is_some());
    }
    assert!(detail.questions[0].turns[13].is_continuation());
    assert!(detail.questions[0].turns[14].is_continuation());

    // Question 2: 4 turns
    assert_eq!(detail.questions[1].turns.len(), 4);
    assert_eq!(
        detail.questions[1].question.title.as_deref(),
        Some("Now implement the cache layer for query results."),
        "Question 2 title must be from second user turn"
    );
    assert!(!detail.questions[1].turns[0].is_subagent());
    for t in &detail.questions[1].turns[1..=3] {
        assert!(t.is_subagent());
        assert_eq!(t.turn_kind.as_deref(), Some("subagent"));
        assert_eq!(t.agent_role.as_deref(), Some("cache-architect"));
    }

    // Question 3: 1 turn
    assert_eq!(detail.questions[2].turns.len(), 1);
    assert_eq!(
        detail.questions[2].question.title.as_deref(),
        Some("Are all performance targets met?"),
        "Question 3 title must be from third user turn"
    );
    assert!(!detail.questions[2].turns[0].is_subagent());

    // 5. Verify list_conversation_question_details_sqlx matches
    let q_details =
        super::list_conversation_question_details_sqlx(&pool, "default", session_id, None, 10, 0)
            .await
            .unwrap();
    assert_eq!(q_details.len(), 3);
    assert_eq!(q_details[0].turns.len(), 15);
    assert_eq!(q_details[1].turns.len(), 4);
    assert_eq!(q_details[2].turns.len(), 1);
}
