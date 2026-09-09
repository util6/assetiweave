#[cfg(test)]
mod tests {
    use crate::backend::{
        agents::types::AgentProtocol,
        ai_execution::{
            executor::BackendFuture, AgentExecutionRuntime, AiExecutionRequest, AiExecutionResult,
        },
        application::{
            session_memory::{build_evidence_references, build_session_memory_prompt},
            AppService,
        },
        memory_redaction::redact_memory_text,
        models::{
            BoundedMemoryBudgetPolicy, ConversationAdapter, ConversationAdapterKind,
            ConversationAdapterTrustState, ConversationPartKind, ConversationPartRole,
            ConversationSource, ConversationSourceKind, MemoryScope, NormalizedConversationPart,
            NormalizedConversationSession, NormalizedConversationTurn,
        },
        store,
    };
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    struct FakeRuntime {
        result_text: Mutex<String>,
        requests: Mutex<Vec<AiExecutionRequest>>,
    }

    impl FakeRuntime {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                result_text: Mutex::new("{}".to_string()),
                requests: Mutex::new(Vec::new()),
            })
        }
    }

    impl AgentExecutionRuntime for FakeRuntime {
        fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
            let result_text = self.result_text.lock().expect("fake result lock").clone();
            self.requests
                .lock()
                .expect("fake request lock")
                .push(request.clone());
            Box::pin(async move {
                Ok(AiExecutionResult {
                    text: result_text,
                    agent_id: request.agent_id,
                    protocol: AgentProtocol::Acp,
                    requested_model: request.model,
                    elapsed_ms: 1,
                    persistent_binding: None,
                    replay_text: None,
                })
            })
        }
    }

    struct FixtureHarness {
        root: PathBuf,
        adapter: ConversationAdapter,
        source: ConversationSource,
    }

    impl FixtureHarness {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "assetiweave-baseline-{}-{}",
                name,
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(&root).expect("create baseline fixture root");
            let timestamp = "2026-09-09T00:00:00Z";
            let adapter = ConversationAdapter {
                id: format!("adapter-{}", name),
                name: format!("Adapter {}", name),
                kind: ConversationAdapterKind::External,
                version: "1.0.0".to_string(),
                enabled: true,
                manifest_path: None,
                executable_path: None,
                content_hash: None,
                trusted_hash: None,
                trust_state: ConversationAdapterTrustState::Trusted,
                protocol_version: Some(1),
                capabilities: vec!["read_session".to_string()],
                input_kinds: vec![ConversationSourceKind::Directory],
                card_contract_version: None,
                card_kinds: Vec::new(),
                created_at: timestamp.to_string(),
                updated_at: timestamp.to_string(),
            };
            let source = ConversationSource {
                id: format!("source-{}", name),
                adapter_id: adapter.id.clone(),
                name: format!("Source {}", name),
                kind: ConversationSourceKind::Directory,
                location: root.to_string_lossy().to_string(),
                config_json: None,
                enabled: true,
                last_synced_at: None,
                last_sync_status: None,
                created_at: timestamp.to_string(),
                updated_at: timestamp.to_string(),
            };
            Self {
                root,
                adapter,
                source,
            }
        }

        async fn import_session(
            &self,
            pool: &sqlx::SqlitePool,
            session: NormalizedConversationSession,
        ) -> String {
            store::upsert_conversation_adapter_sqlx(pool, "default", &self.adapter)
                .await
                .expect("upsert adapter");
            store::upsert_conversation_source_sqlx(pool, "default", &self.source)
                .await
                .expect("upsert source");
            let external_id = session.external_id.clone();
            store::import_conversation_sessions_sqlx(
                pool,
                "default",
                &self.source,
                &[session],
                false,
            )
            .await
            .expect("import session");
            let session_id: String = sqlx::query_scalar(
                "SELECT id FROM conversation_sessions WHERE tenant_id = 'default' AND external_id = ?1",
            )
            .bind(&external_id)
            .fetch_one(pool)
            .await
            .expect("fetch session id");
            session_id
        }
    }

    /// Fixture 1: 工具与日志密集型长会话（包含大量冗余日志输出）
    fn make_heavy_log_session() -> NormalizedConversationSession {
        let timestamp = "2026-09-09T01:00:00Z";
        let mut turns = Vec::new();
        for index in 0..40 {
            let user_text = format!("Execute benchmark build step {}", index);
            let log_chunk = format!(
                "Compiling package-xyz v0.{}.0 (/path/to/project)\nRunning rustc with flags --cfg=test --edition=2021\nStandard output stream:\n{}Finished dev [unoptimized + debuginfo] target(s) in 0.42s",
                index,
                " [LOG TRACE ENTRY: checksum=abcdef0123456789 byte_offset=10240 status=READY thread_id=worker-4] \n".repeat(25)
            );
            turns.push(NormalizedConversationTurn {
                external_id: format!("turn-heavy-{}", index),
                turn_index: index as i64,
                user_text,
                title: None,
                started_at: Some(timestamp.to_string()),
                ended_at: Some(timestamp.to_string()),
                parts: vec![NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Text,
                    text: Some(log_chunk),
                    language: None,
                    command: Some(format!("cargo build --step-{}", index)),
                    cwd: Some("/path/to/project".to_string()),
                    status: Some("success".to_string()),
                    exit_code: Some(0),
                    command_label: None,
                    source_execution_id: None,
                    content_card: None,
                    metadata_json: Some(
                        r#"{"content_card":{"type":"answer","format":"markdown"}}"#.to_string(),
                    ),
                }],
            });
        }
        NormalizedConversationSession {
            external_id: "heavy-log-session".to_string(),
            title: Some("Heavy Log & Tool Output Benchmark Session".to_string()),
            project_path: Some("/path/to/project".to_string()),
            started_at: Some(timestamp.to_string()),
            updated_at: Some(timestamp.to_string()),
            source_locator: Some("fixture://heavy-log".to_string()),
            source_fingerprint: Some("rev-heavy-1".to_string()),
            turns,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn baseline_unbounded_evidence_violates_budget_and_demonstrates_need_for_bounded_pack() {
        let harness = FixtureHarness::new("heavy");
        let db_path = harness.root.join("app.db");
        let fake = FakeRuntime::new();
        let service = AppService::open_with_db_path_and_runtime(db_path, fake)
            .await
            .expect("open app service");

        let pool = service.db.pool().clone();
        let session = make_heavy_log_session();
        let session_id = harness.import_session(&pool, session).await;
        let detail = store::load_conversation_session_detail_sqlx(&pool, "default", &session_id)
            .await
            .expect("load detail");

        let evidence = build_evidence_references(&detail);
        let prompt = build_session_memory_prompt(&detail, &evidence).expect("build prompt");

        let budget = BoundedMemoryBudgetPolicy::default();

        assert!(
            evidence.len() >= 40,
            "Old version collects all nodes without selective packing"
        );

        let prompt_chars = prompt.chars().count();
        println!(
            "[E0 Baseline Measurement] Heavy log session prompt length: {} chars, evidence count: {}",
            prompt_chars,
            evidence.len()
        );

        assert!(
            prompt_chars > budget.initial_pack_max_chars,
            "Baseline proves that unpruned prompt ({} chars) exceeds initial pack budget ({} chars)",
            prompt_chars,
            budget.initial_pack_max_chars
        );
    }

    #[test]
    fn baseline_redaction_precision_gap_identifies_git_sha_over_redaction() {
        let git_sha = "bc5c14e1234567890abcdef1234567890abcdef1";
        let result = redact_memory_text(git_sha);

        println!(
            "[E0 Baseline Measurement] Current redaction result for Git SHA: {}",
            result.text
        );
        assert!(
            result.text.contains("[REDACTED:high_entropy]"),
            "Baseline confirms that current high_entropy regex over-redacts 40-char git commit SHA"
        );
    }

    #[tokio::test]
    async fn test_internal_source_isolation_hides_agent_sessions_from_views_and_memory() {
        let harness = FixtureHarness::new("isolation");
        let db_path = harness.root.join("app.db");
        let fake = FakeRuntime::new();
        let service = AppService::open_with_db_path_and_runtime(db_path, fake)
            .await
            .expect("open app service");
        let pool = service.db.pool().clone();

        // 1. 构造一个普通用户会话
        let user_session = NormalizedConversationSession {
            external_id: "user-session-1".to_string(),
            title: Some("User Conversation".to_string()),
            project_path: Some("/Users/test/project".to_string()),
            started_at: Some("2026-09-09T10:00:00Z".to_string()),
            updated_at: Some("2026-09-09T10:05:00Z".to_string()),
            source_locator: Some("file:///test/user.json".to_string()),
            source_fingerprint: Some("fp-user-1".to_string()),
            execution_origin: Some("user".to_string()),
            execution_purpose: None,
            user_visible: Some(true),
            turns: vec![NormalizedConversationTurn {
                external_id: "turn-user-1".to_string(),
                turn_index: 0,
                user_text: "How to configure auth?".to_string(),
                title: None,
                started_at: Some("2026-09-09T10:00:00Z".to_string()),
                ended_at: Some("2026-09-09T10:01:00Z".to_string()),
                parts: vec![NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Text,
                    text: Some("Here is how to configure auth...".to_string()),
                    command: None,
                    cwd: None,
                    status: None,
                    exit_code: None,
                    language: None,
                    command_label: None,
                    source_execution_id: None,
                    content_card: None,
                    metadata_json: None,
                }],
            }],
        };

        // 2. 构造一个内部 Agent 会话（如 Recall 或 internal memory 执行）
        let internal_session = NormalizedConversationSession {
            external_id: "internal-session-1".to_string(),
            title: Some("Internal Agent Memory Recall".to_string()),
            project_path: Some("/Users/test/project".to_string()),
            started_at: Some("2026-09-09T10:10:00Z".to_string()),
            updated_at: Some("2026-09-09T10:12:00Z".to_string()),
            source_locator: Some("memory-recall://test-1".to_string()),
            source_fingerprint: Some("fp-internal-1".to_string()),
            execution_origin: Some("internal_memory".to_string()),
            execution_purpose: Some("recall".to_string()),
            user_visible: Some(false),
            turns: vec![NormalizedConversationTurn {
                external_id: "turn-internal-1".to_string(),
                turn_index: 0,
                user_text: "What did user work on recently?".to_string(),
                title: None,
                started_at: Some("2026-09-09T10:10:00Z".to_string()),
                ended_at: Some("2026-09-09T10:11:00Z".to_string()),
                parts: vec![NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Text,
                    text: Some("User was configuring auth...".to_string()),
                    command: None,
                    cwd: None,
                    status: None,
                    exit_code: None,
                    language: None,
                    command_label: None,
                    source_execution_id: None,
                    content_card: None,
                    metadata_json: None,
                }],
            }],
        };

        store::upsert_conversation_adapter_sqlx(&pool, "default", &harness.adapter)
            .await
            .expect("upsert adapter");
        store::upsert_conversation_source_sqlx(&pool, "default", &harness.source)
            .await
            .expect("upsert source");
        store::import_conversation_sessions_sqlx(
            &pool,
            "default",
            &harness.source,
            &[user_session, internal_session],
            false,
        )
        .await
        .expect("import sessions");

        let user_id: String = sqlx::query_scalar(
            "SELECT id FROM conversation_sessions WHERE tenant_id = 'default' AND external_id = 'user-session-1'",
        )
        .fetch_one(&pool)
        .await
        .expect("fetch user session id");

        let _internal_id: String = sqlx::query_scalar(
            "SELECT id FROM conversation_sessions WHERE tenant_id = 'default' AND external_id = 'internal-session-1'",
        )
        .fetch_one(&pool)
        .await
        .expect("fetch internal session id");

        // 验证 1: 会话列表只能查到用户会话，不能查到内部会话
        let sessions =
            store::list_conversation_sessions_sqlx(&pool, "default", None, None, None, 10, 0)
                .await
                .expect("list conversation sessions");

        assert_eq!(sessions.len(), 1, "Only user sessions should be returned");
        assert_eq!(sessions[0].session.id, user_id);

        // 验证 2: 待提取候选列表 (Session candidates) 只能查到用户会话
        let candidates = store::load_session_candidates_sqlx(
            &pool,
            "default",
            &harness.source.id,
            &[user_id.clone(), _internal_id.clone()],
            "",
        )
        .await
        .expect("load candidates");

        assert_eq!(
            candidates.len(),
            1,
            "Only user sessions can be memory candidates"
        );
        assert_eq!(candidates[0].id, user_id);

        // 验证 3: Recall 候选列表只能查到用户会话的问题，内部 Agent 会话被完全隔离
        let scope = MemoryScope {
            app_id: None,
            source_id: None,
            project_path: None,
            session_id: None,
        };
        let (recall_count, recall_refs) = store::list_memory_recall_question_refs_sqlx(
            &pool, "default", &scope, None, None, true, 10, 0,
        )
        .await
        .expect("list recall refs");

        assert_eq!(
            recall_count, 1,
            "Only user session questions should be in recall scope"
        );
        assert_eq!(recall_refs.len(), 1);
        assert_eq!(recall_refs[0].session_id, user_id);
    }
}
