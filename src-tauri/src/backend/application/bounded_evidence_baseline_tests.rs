#[cfg(test)]
mod tests {
    use crate::backend::{
        application::session_memory::{build_evidence_references, build_session_memory_prompt},
        dto::ConversationSessionDetail,
        memory_redaction::redact_memory_text,
        models::{
            BoundedMemoryBudgetPolicy, ConversationAdapter, ConversationAdapterKind,
            ConversationAdapterTrustState, ConversationPartKind, ConversationPartRole,
            ConversationSource, ConversationSourceKind, NormalizedConversationPart,
            NormalizedConversationSession, NormalizedConversationTurn,
        },
        store,
    };
    use chrono::Utc;
    use std::path::PathBuf;

    /// 构造包含不同场景的测试环境与会话数据
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
            store::upsert_conversation_adapter_sqlx(pool, &self.adapter, Utc::now().to_rfc3339())
                .await
                .expect("upsert adapter");
            store::upsert_conversation_source_sqlx(pool, &self.source, Utc::now().to_rfc3339())
                .await
                .expect("upsert source");
            let imported = store::import_canonical_session_sqlx(
                pool,
                &self.source.id,
                &session,
                Utc::now().to_rfc3339(),
            )
            .await
            .expect("import session");
            imported.session_id
        }
    }

    /// Fixture 1: 工具与日志密集型长会话（包含大量冗余日志输出）
    fn make_heavy_log_session() -> NormalizedConversationSession {
        let timestamp = "2026-09-09T01:00:00Z";
        let mut turns = Vec::new();
        // 创建 40 个包含大量冗余 log 输出的 turn/part
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
                    metadata_json: None,
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
        }
    }

    /// Fixture 2: 用户中途纠正与否决（先提议方案 A，后明确拒绝并更正为方案 B）
    fn make_rejection_and_correction_session() -> NormalizedConversationSession {
        let timestamp = "2026-09-09T02:00:00Z";
        let turns = vec![
            NormalizedConversationTurn {
                external_id: "turn-corr-1".to_string(),
                turn_index: 0,
                user_text: "Can we use an in-memory HashMap to cache Memory records across restarts?".to_string(),
                title: None,
                started_at: Some(timestamp.to_string()),
                ended_at: Some(timestamp.to_string()),
                parts: vec![NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Text,
                    text: Some("Yes, we can store all Memory models in a global lazy_static HashMap in memory.".to_string()),
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
            },
            NormalizedConversationTurn {
                external_id: "turn-corr-2".to_string(),
                turn_index: 1,
                user_text: "No, absolutely not. Contract C-A02 strictly requires SQLite as the only structured authority. We reject in-memory HashMap and require persisting to SQLite!".to_string(),
                title: None,
                started_at: Some(timestamp.to_string()),
                ended_at: Some(timestamp.to_string()),
                parts: vec![NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Text,
                    text: Some("Understood. We explicitly reject the in-memory HashMap proposal and confirm SQLite as the single authority.".to_string()),
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
            },
        ];
        NormalizedConversationSession {
            external_id: "correction-session".to_string(),
            title: Some("User Rejection & Architecture Correction Session".to_string()),
            project_path: Some("/path/to/project".to_string()),
            started_at: Some(timestamp.to_string()),
            updated_at: Some(timestamp.to_string()),
            source_locator: Some("fixture://correction".to_string()),
            source_fingerprint: Some("rev-corr-1".to_string()),
            turns,
        }
    }

    /// Fixture 3: 包含凭据、Git SHA (40-hex)、长路径混合的样本
    fn make_credentials_and_sha_session() -> NormalizedConversationSession {
        let timestamp = "2026-09-09T03:00:00Z";
        let text = concat!(
            "Configuration setup:\n",
            "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.secretpayload.1234567890\n",
            "OpenAI API key: sk-proj-1234567890abcdefghijklmnopqrstuvwxyz\n",
            "Git revision commit: bc5c14e1234567890abcdef1234567890abcdef1\n",
            "Source file path: /Users/util6/code-space/assetiweave/src-tauri/src/backend/session_memory.rs\n",
            "Test symbol: test_phase1_worker_honors_idle_boundary_persists_redacted_output\n",
        );
        NormalizedConversationSession {
            external_id: "credentials-sha-session".to_string(),
            title: Some("Credentials, Git SHA, and Long Path Session".to_string()),
            project_path: Some("/Users/util6/code-space/assetiweave".to_string()),
            started_at: Some(timestamp.to_string()),
            updated_at: Some(timestamp.to_string()),
            source_locator: Some("fixture://creds".to_string()),
            source_fingerprint: Some("rev-creds-1".to_string()),
            turns: vec![NormalizedConversationTurn {
                external_id: "turn-creds-1".to_string(),
                turn_index: 0,
                user_text: "Configure the deployment pipeline with credentials and git revision"
                    .to_string(),
                title: None,
                started_at: Some(timestamp.to_string()),
                ended_at: Some(timestamp.to_string()),
                parts: vec![NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Text,
                    text: Some(text.to_string()),
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
        }
    }

    #[tokio::test]
    async fn baseline_unbounded_evidence_violates_budget_and_demonstrates_need_for_bounded_pack() {
        let harness = FixtureHarness::new("heavy");
        let pool = sqlx::SqlitePool::connect(":memory:")
            .await
            .expect("connect sqlite");
        store::run_database_migrations_sqlx(&pool)
            .await
            .expect("migrations");

        let session = make_heavy_log_session();
        let session_id = harness.import_session(&pool, session).await;
        let detail = store::get_conversation_session_detail_sqlx(&pool, "default", &session_id)
            .await
            .expect("get detail")
            .expect("session exists");

        // 测量旧版 build_evidence_references 与 build_session_memory_prompt
        let evidence = build_evidence_references(&detail);
        let prompt = build_session_memory_prompt(&detail, &evidence).expect("build prompt");

        let budget = BoundedMemoryBudgetPolicy::default();

        // 1. 证据数量：旧版全量选择所有 nodes (40 个)
        assert!(
            evidence.len() >= 40,
            "Old version collects all nodes without selective packing"
        );

        // 2. Prompt 体积测量：包含所有 log 重复行，总字符数极大
        let prompt_chars = prompt.chars().count();
        println!(
            "[E0 Baseline Measurement] Heavy log session prompt length: {} chars, evidence count: {}",
            prompt_chars,
            evidence.len()
        );

        // 关键断言（Red 证据）：旧版未裁剪 Prompt 远超首包预算 32,000 字符！
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

        // 当前现存实现将 40 位十六进制 Git SHA 误判为 high entropy secret
        // 这证实了 08 规范所指出的：“高熵脱敏还可能误伤路径、符号和 Git SHA。E2 切片将解决该问题。”
        println!(
            "[E0 Baseline Measurement] Current redaction result for Git SHA: {}",
            result.text
        );
        assert!(
            result.text.contains("[REDACTED:high_entropy]"),
            "Baseline confirms that current high_entropy regex over-redacts 40-char git commit SHA"
        );
    }
}
