use super::*;
use crate::backend::{
    agents::types::AgentProtocol,
    ai_execution::{
        executor::BackendFuture, AgentExecutionRuntime, AiExecutionRequest, AiExecutionResult,
    },
    models::{
        ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState,
        ConversationPartKind, ConversationPartRole, ConversationSource, ConversationSourceKind,
        NormalizedConversationPart, NormalizedConversationSession, NormalizedConversationTurn,
    },
};
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

    fn set_result(&self, result_text: String) {
        *self.result_text.lock().expect("fake result lock") = result_text;
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
                session_cleanup: SessionCleanupStatus::Deleted,
            })
        })
    }
}

#[test]
fn completion_signal_is_provider_neutral_and_requires_an_explicit_value() {
    let completed = serde_json::json!({ "session_status": "completed" });
    let pending = serde_json::json!({ "status": "completed-command" });
    assert!(value_marks_completion(&completed));
    assert!(!value_marks_completion(&pending));
}

#[test]
fn unix_activity_timestamps_reach_the_idle_gate() {
    let last_activity = DateTime::parse_from_rfc3339("2026-08-31T10:00:00Z")
        .expect("parse last activity")
        .with_timezone(&Utc);
    let now = last_activity + Duration::minutes(30);
    for updated_at in [
        last_activity.timestamp().to_string(),
        last_activity.timestamp_millis().to_string(),
    ] {
        let detail = ConversationSessionDetail {
            session: crate::backend::models::ConversationSession {
                id: "session".to_string(),
                source_id: "source".to_string(),
                adapter_id: "adapter".to_string(),
                external_id: "external".to_string(),
                title: "Session".to_string(),
                project_path: None,
                started_at: None,
                updated_at: Some(updated_at),
                source_locator: None,
                source_fingerprint: None,
                missing: false,
                created_at: now.to_rfc3339(),
                imported_at: now.to_rfc3339(),
                execution_origin: "user".to_string(),
                execution_purpose: None,
                user_visible: true,
            },
            questions: Vec::new(),
        };
        assert!(session_idle_ready(&detail, now));
    }
}

#[test]
fn json_fence_is_removed_without_touching_payload() {
    assert_eq!(strip_json_fence("```json\n{\"a\":1}\n```"), "{\"a\":1}");
    assert_eq!(strip_json_fence("```\n{\"a\":1}\n```"), "{\"a\":1}");
}

#[test]
fn validation_promotes_valid_event_evidence_into_persisted_references() {
    let job = SessionMemoryJob {
        tenant_id: "default".to_string(),
        id: "job-event-reference".to_string(),
        session_id: "session-event-reference".to_string(),
        source_id: "source-event-reference".to_string(),
        source_revision: 7,
        source_fingerprint: "fingerprint-event-reference".to_string(),
        contract_version: SESSION_MEMORY_CONTRACT_VERSION.to_string(),
        prompt_version: SESSION_MEMORY_PROMPT_VERSION.to_string(),
        ownership_token: Some("owner-event-reference".to_string()),
        ..Default::default()
    };
    let output = SessionMemoryAgentOutput {
        summary: "The session produced a verified deliverable.".to_string(),
        goal: "Produce the deliverable".to_string(),
        result: "Completed".to_string(),
        decisions: Vec::new(),
        verification: Vec::new(),
        blockers: Vec::new(),
        follow_up: Vec::new(),
        topics: vec!["delivery".to_string()],
        source_references: vec![AgentSourceReference {
            reference_key: "ref-summary".to_string(),
        }],
        events: vec![AgentRecentEvent {
            category: "verification".to_string(),
            title: "Deliverable verified".to_string(),
            summary: "The verification evidence confirms the output.".to_string(),
            occurred_at: None,
            source_reference: Some("ref-event-only".to_string()),
            fingerprint: None,
        }],
    };
    let evidence = vec![
        EvidenceReference {
            key: "ref-summary".to_string(),
            locator: ConversationContentNodeLocator {
                question_id: "question-summary".to_string(),
                turn_id: "turn-summary".to_string(),
                part_id: "part-summary".to_string(),
                node_order: 0,
            },
            node_id: Some("node-summary".to_string()),
            content: "Summary evidence".to_string(),
        },
        EvidenceReference {
            key: "ref-event-only".to_string(),
            locator: ConversationContentNodeLocator {
                question_id: "question-event".to_string(),
                turn_id: "turn-event".to_string(),
                part_id: "part-event".to_string(),
                node_order: 1,
            },
            node_id: Some("node-event".to_string()),
            content: "Event evidence".to_string(),
        },
    ];

    let persist = validated_persist_input(&job, &output, &evidence, None, "2026-09-17T00:00:00Z")
        .expect("event-only bounded evidence should be promoted to a canonical reference");

    assert_eq!(
        persist
            .references
            .iter()
            .map(|reference| reference.reference_key.as_str())
            .collect::<Vec<_>>(),
        vec!["ref-summary", "ref-event-only"]
    );
    assert!(persist.events[0].source_reference_id.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn phase1_worker_honors_idle_boundary_persists_redacted_output_and_is_idempotent() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-session-memory-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create session memory fixture root");
    let db_path = root.join("app.db");
    let fake = FakeRuntime::new();
    let service = AppService::open_with_db_path_and_runtime(db_path.clone(), fake.clone())
        .await
        .expect("open app service with fake agent");
    let timestamp = "2026-08-30T23:00:00Z";
    let adapter = ConversationAdapter {
        id: "session-memory-fixture-adapter".to_string(),
        name: "Session Memory Fixture Agent".to_string(),
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
        id: "session-memory-fixture-source".to_string(),
        adapter_id: adapter.id.clone(),
        name: "Session Memory Fixture Source".to_string(),
        kind: ConversationSourceKind::Directory,
        location: root.to_string_lossy().to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: timestamp.to_string(),
        updated_at: timestamp.to_string(),
    };
    let session = NormalizedConversationSession {
        external_id: "session-memory-fixture".to_string(),
        title: Some("Session Memory Fixture".to_string()),
        project_path: None,
        started_at: Some("2026-08-30T22:00:00Z".to_string()),
        updated_at: Some(timestamp.to_string()),
        source_locator: Some("fixture://session-memory".to_string()),
        source_fingerprint: Some("fixture-revision-1".to_string()),
        turns: vec![NormalizedConversationTurn {
            external_id: "turn-1".to_string(),
            turn_index: 0,
            user_text: "Implement the Session Memory fixture".to_string(),
            title: None,
            started_at: Some(timestamp.to_string()),
            ended_at: Some(timestamp.to_string()),
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some("The fixture was implemented.".to_string()),
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
    let pool = service.db.pool().clone();
    let source_for_import = source.clone();
    let adapter_for_import = adapter.clone();
    crate::backend::store::upsert_conversation_adapter_sqlx(&pool, "default", &adapter_for_import)
        .await
        .expect("upsert adapter fixture");
    crate::backend::store::upsert_conversation_source_sqlx(&pool, "default", &source_for_import)
        .await
        .expect("upsert source fixture");
    crate::backend::store::import_conversation_sessions_sqlx(
        &pool,
        "default",
        &source_for_import,
        &[session],
        false,
    )
    .await
    .expect("import canonical conversation fixture");

    let session_id: String = sqlx::query_scalar(
            "SELECT id FROM conversation_sessions WHERE tenant_id = 'default' AND external_id = 'session-memory-fixture'",
        )
        .fetch_one(service.db.pool())
        .await
        .expect("load imported session id");
    let detail = crate::backend::store::load_conversation_session_detail_sqlx(
        service.db.pool(),
        "default",
        &session_id,
    )
    .await
    .expect("load canonical session detail");
    let reference_key = detail.questions[0]
        .projected_content_nodes
        .first()
        .map(|node| format!("node:{}", node.node_id))
        .unwrap_or_else(|| format!("turn:{}", detail.questions[0].turns[0].id));
    let secret = "ghp_12345678901234567890";
    let events = RecentMemoryEventCategory::ALL
        .iter()
        .enumerate()
        .map(|(index, category)| {
            json!({
                "category": category.as_str(),
                "title": format!("Event {index}"),
                "summary": format!("Event summary {index}"),
                "source_reference": reference_key.clone(),
                "fingerprint": format!("event-{index}"),
            })
        })
        .collect::<Vec<_>>();
    fake.set_result(
        json!({
            "summary": format!("Completed with {secret}"),
            "goal": "Create a revision-bound memory",
            "result": "Fixture persisted",
            "decisions": ["Use canonical Conversation evidence"],
            "verification": ["Six Recent Event categories validated"],
            "blockers": [],
            "follow_up": ["Review the generated locator"],
            "topics": ["memory"],
            "source_references": [{ "reference_key": reference_key.clone() }],
            "events": events,
        })
        .to_string(),
    );
    let now = DateTime::parse_from_rfc3339("2026-08-30T23:00:00Z")
        .expect("parse controlled clock")
        .with_timezone(&Utc);
    assert_eq!(
        service
            .enqueue_session_memory_jobs_at(
                &source.id,
                "sync-session-memory",
                1,
                "event-session-memory",
                Some(std::slice::from_ref(&session_id)),
                now,
            )
            .await
            .expect("enqueue phase1 job"),
        1
    );
    let job_id: String = sqlx::query_scalar(
        "SELECT id FROM session_memory_jobs WHERE tenant_id = 'default' AND session_id = ?1",
    )
    .bind(&session_id)
    .fetch_one(service.db.pool())
    .await
    .expect("load phase1 job id");
    assert!(service
        .run_session_memory_phase1_at(&job_id, now + Duration::minutes(29) + Duration::seconds(59),)
        .await
        .expect("idle boundary before deadline")
        .is_none());
    let memory = service
        .run_session_memory_phase1_at(&job_id, now + Duration::minutes(30))
        .await
        .expect("run phase1 worker")
        .expect("phase1 memory result");
    assert_eq!(memory.source_revision, 1);
    assert!(!memory.summary.contains(secret));
    assert!(memory.summary.contains("[REDACTED:api_key]"));

    sqlx::query(
            "UPDATE conversation_sessions SET source_fingerprint = 'phase1-fixture-v2' WHERE tenant_id = 'default' AND id = ?1",
        )
        .bind(&session_id)
        .execute(service.db.pool())
        .await
        .expect("advance fixture fingerprint");
    assert_eq!(
        service
            .enqueue_session_memory_jobs_at(
                &source.id,
                "sync-session-memory-scheduler",
                2,
                "event-session-memory-scheduler",
                Some(std::slice::from_ref(&session_id)),
                now + Duration::minutes(30),
            )
            .await
            .expect("enqueue scheduler phase1 job"),
        1
    );
    let scheduled_job_id: String = sqlx::query_scalar(
            "SELECT id FROM session_memory_jobs WHERE tenant_id = 'default' AND session_id = ?1 AND source_revision = 2",
        )
        .bind(&session_id)
        .fetch_one(service.db.pool())
        .await
        .expect("load scheduler job id");
    let mut settings = service.app_settings_value();
    settings["memory"]["watermarkTime1"] = json!("02:00");
    settings["memory"]["watermarkTime2"] = json!("23:15");
    service
        .save_app_settings(settings)
        .await
        .expect("place the controlled session inside the latest Recent watermark");
    let scheduled_task_id = format!("session-memory-{}", scheduled_job_id);
    assert_eq!(
        service
            .reconcile_session_memory_jobs_for_tenant_at("default", now + Duration::minutes(30))
            .await
            .expect("schedule durable phase1 job"),
        1
    );
    for _ in 0..100 {
        let status: String = sqlx::query_scalar(
            "SELECT status FROM session_memory_jobs WHERE tenant_id = 'default' AND id = ?1",
        )
        .bind(&scheduled_job_id)
        .fetch_one(service.db.pool())
        .await
        .expect("read scheduled job status");
        if status == "succeeded" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let scheduled_status: String = sqlx::query_scalar(
        "SELECT status FROM session_memory_jobs WHERE tenant_id = 'default' AND id = ?1",
    )
    .bind(&scheduled_job_id)
    .fetch_one(service.db.pool())
    .await
    .expect("read completed scheduled job");
    assert_eq!(scheduled_status, "succeeded");
    let mut scheduled_task = None;
    for _ in 0..100 {
        if let Some(snapshot) = service.runtime.task_runtime().get(&scheduled_task_id) {
            if snapshot.progress.as_ref().map(|value| value.current) == Some(3) {
                scheduled_task = Some(snapshot);
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let scheduled_task = scheduled_task.expect("read scheduled TaskRuntime projection");
    assert_eq!(
        scheduled_task.progress.as_ref().map(|value| value.current),
        Some(3)
    );
    assert_eq!(scheduled_task.stages.len(), 6);
    assert_eq!(scheduled_task.stages[0].id, "claim");
    assert_eq!(scheduled_task.stages[0].status, StageStatus::Succeeded);
    assert_eq!(scheduled_task.stages[1].id, "load_facts");
    assert_eq!(scheduled_task.stages[1].status, StageStatus::Succeeded);
    assert_eq!(scheduled_task.stages[2].id, "agent_execution");
    assert_eq!(scheduled_task.stages[2].status, StageStatus::Succeeded);
    assert_eq!(scheduled_task.stages[3].id, "validation");
    assert_eq!(scheduled_task.stages[3].status, StageStatus::Succeeded);
    assert_eq!(scheduled_task.stages[4].id, "publish");
    assert_eq!(scheduled_task.stages[4].status, StageStatus::Succeeded);
    assert_eq!(scheduled_task.stages[5].id, "cleanup_session");
    assert_eq!(scheduled_task.stages[5].status, StageStatus::Succeeded);
    assert_eq!(scheduled_task.outcome, Some(TaskOutcome::Success));

    let mut recent_job_count = 0_i64;
    for _ in 0..100 {
        recent_job_count = sqlx::query_scalar(
            "SELECT COUNT(*) FROM recent_memory_jobs WHERE tenant_id = 'default'",
        )
        .fetch_one(service.db.pool())
        .await
        .expect("count Recent jobs after Phase-1 terminal state");
        if recent_job_count > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        recent_job_count, 1,
        "the terminal Phase-1 task must immediately reconcile its dependent Recent snapshot"
    );
    for _ in 0..100 {
        let status: String = sqlx::query_scalar(
            "SELECT status FROM recent_memory_jobs WHERE tenant_id = 'default' LIMIT 1",
        )
        .fetch_one(service.db.pool())
        .await
        .expect("read dependent Recent job status");
        if !matches!(status.as_str(), "queued" | "running") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let jobs =
        crate::backend::store::count_session_memory_rows_sqlx(service.db.pool(), "default", "jobs")
            .await
            .expect("count jobs");
    let memories = crate::backend::store::count_session_memory_rows_sqlx(
        service.db.pool(),
        "default",
        "memories",
    )
    .await
    .expect("count memories");
    let references = crate::backend::store::count_session_memory_rows_sqlx(
        service.db.pool(),
        "default",
        "references",
    )
    .await
    .expect("count references");
    let events = crate::backend::store::count_session_memory_rows_sqlx(
        service.db.pool(),
        "default",
        "events",
    )
    .await
    .expect("count events");
    let row_counts = (jobs, memories, references, events);
    assert_eq!(row_counts, (2, 2, 2, 12));
    let raw_output: String = sqlx::query_scalar(
            "SELECT raw_output_json FROM session_memories WHERE tenant_id = 'default' AND session_id = ?1",
        )
        .bind(&session_id)
        .fetch_one(service.db.pool())
        .await
        .expect("read sanitized phase1 output");
    assert!(!raw_output.contains(secret));
    assert!(raw_output.contains("[REDACTED:api_key]"));
    assert_eq!(
        service
            .enqueue_session_memory_jobs_at(
                &source.id,
                "sync-session-memory-replay",
                1,
                "event-session-memory-replay",
                Some(std::slice::from_ref(&session_id)),
                now + Duration::minutes(31),
            )
            .await
            .expect("replay phase1 event"),
        0
    );
    let recent = service
        .list_recent_conversation_sessions_at(
            crate::backend::application::RecentConversationSessionListParams::default(),
            now + Duration::minutes(31),
        )
        .await
        .expect("read Recent projection");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].recent_events.len(), 6);
    let requests = fake.requests.lock().expect("fake request lock");
    let session_requests = requests
        .iter()
        .filter(|request| request.purpose == AiExecutionPurpose::SessionMemory)
        .collect::<Vec<_>>();
    assert_eq!(session_requests.len(), 2);
    assert!(!session_requests[0].prompt.contains(secret));
    drop(requests);
    drop(service);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "multi_thread")]
async fn session_memory_uses_its_own_runtime_settings_snapshot() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-session-memory-settings-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create test root");
    let db_a_path = root.join("app_a.db");
    let db_b_path = root.join("app_b.db");

    let fake = FakeRuntime::new();
    fake.set_result(
        serde_json::json!({
            "summary": "Phase 1 summary",
            "topics": ["settings"],
            "source_references": [],
            "events": [],
        })
        .to_string(),
    );

    let service_a = AppService::open_with_db_path_and_runtime(db_a_path.clone(), fake.clone())
        .await
        .expect("open service A");

    let service_b = AppService::open_with_db_path_and_runtime(db_b_path.clone(), fake.clone())
        .await
        .expect("open service B");

    let timestamp = "2026-08-30T23:00:00Z";
    let now = DateTime::parse_from_rfc3339(timestamp)
        .expect("parse time")
        .with_timezone(&Utc);

    async fn setup_job(
        service: &AppService,
        session_id: &str,
        now: DateTime<Utc>,
    ) -> (String, String) {
        let pool = service.db.pool();
        let adapter = ConversationAdapter {
            id: format!("adapter-{session_id}"),
            name: "Adapter".to_string(),
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
            created_at: "2026-08-30T23:00:00Z".to_string(),
            updated_at: "2026-08-30T23:00:00Z".to_string(),
        };
        let source = ConversationSource {
            id: format!("source-{session_id}"),
            adapter_id: adapter.id.clone(),
            name: "Source".to_string(),
            kind: ConversationSourceKind::Directory,
            location: "/fixture".to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: "2026-08-30T23:00:00Z".to_string(),
            updated_at: "2026-08-30T23:00:00Z".to_string(),
        };
        let session = NormalizedConversationSession {
            external_id: session_id.to_string(),
            title: Some("Session".to_string()),
            project_path: None,
            started_at: Some("2026-08-30T22:00:00Z".to_string()),
            updated_at: Some("2026-08-30T23:00:00Z".to_string()),
            source_locator: Some("fixture://session".to_string()),
            source_fingerprint: Some("rev1".to_string()),
            turns: vec![NormalizedConversationTurn {
                external_id: "turn-1".to_string(),
                turn_index: 0,
                user_text: "Hello".to_string(),
                title: None,
                started_at: Some("2026-08-30T23:00:00Z".to_string()),
                ended_at: Some("2026-08-30T23:00:00Z".to_string()),
                parts: vec![NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Text,
                    text: Some("World".to_string()),
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
        crate::backend::store::upsert_conversation_adapter_sqlx(pool, "default", &adapter)
            .await
            .unwrap();
        crate::backend::store::upsert_conversation_source_sqlx(pool, "default", &source)
            .await
            .unwrap();
        crate::backend::store::import_conversation_sessions_sqlx(
            pool,
            "default",
            &source,
            &[session],
            false,
        )
        .await
        .unwrap();

        let internal_session_id: String = sqlx::query_scalar(
            "SELECT id FROM conversation_sessions WHERE tenant_id = 'default' AND external_id = ?1",
        )
        .bind(session_id)
        .fetch_one(pool)
        .await
        .unwrap();

        service
            .enqueue_session_memory_jobs_at(
                &source.id,
                "sync-1",
                1,
                "evt-1",
                Some(&[internal_session_id.clone()]),
                now,
            )
            .await
            .unwrap();
        let job_id: String = sqlx::query_scalar(
            "SELECT id FROM session_memory_jobs WHERE tenant_id = 'default' AND session_id = ?1",
        )
        .bind(&internal_session_id)
        .fetch_one(pool)
        .await
        .unwrap();

        let detail = crate::backend::store::load_conversation_session_detail_sqlx(
            pool,
            "default",
            &internal_session_id,
        )
        .await
        .unwrap();
        let reference_key = detail.questions[0]
            .projected_content_nodes
            .first()
            .map(|node| format!("node:{}", node.node_id))
            .unwrap_or_else(|| format!("turn:{}", detail.questions[0].turns[0].id));

        (job_id, reference_key)
    }

    let (job_a_id, _ref_a) = setup_job(&service_a, "session-a", now).await;
    let (job_b_id, ref_b) = setup_job(&service_b, "session-b", now).await;

    fake.set_result(
        serde_json::json!({
            "summary": "Phase 1 summary",
            "topics": ["settings"],
            "source_references": [{ "reference_key": ref_b }],
            "events": [],
        })
        .to_string(),
    );

    let mut settings_a = service_a.app_settings_value();
    settings_a["memory"]["generationEnabled"] = serde_json::json!(false);
    service_a.runtime.update_app_settings_value(settings_a);

    let mut settings_b = service_b.app_settings_value();
    settings_b["memory"]["generationEnabled"] = serde_json::json!(true);
    service_b.runtime.update_app_settings_value(settings_b);

    let run_at = now + Duration::minutes(30);

    let result_a = service_a
        .run_session_memory_phase1_at(&job_a_id, run_at)
        .await
        .expect("phase1 on A");
    assert!(result_a.is_none(), "service A should have cancelled job");
    let status_a: String =
        sqlx::query_scalar("SELECT status FROM session_memory_jobs WHERE id = ?1")
            .bind(&job_a_id)
            .fetch_one(service_a.db.pool())
            .await
            .unwrap();
    assert_eq!(status_a, "canceled", "job A must be canceled in DB_A");

    let result_b = service_b
        .run_session_memory_phase1_at(&job_b_id, run_at)
        .await
        .expect("phase1 on B");
    assert!(result_b.is_some(), "service B should have executed phase 1");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn strip_json_fence_extracts_outermost_json_with_surrounding_text() {
    let input = "Here is the memory:\n```json\n{\"summary\": \"test\"}\n```\nHope this helps!";
    assert_eq!(strip_json_fence(input), "{\"summary\": \"test\"}");

    let input_no_fence = "Prefix text {\"key\": \"value\"} trailing text";
    assert_eq!(strip_json_fence(input_no_fence), "{\"key\": \"value\"}");

    let input_clean = "{\"pure\": 123}";
    assert_eq!(strip_json_fence(input_clean), "{\"pure\": 123}");

    let input_multiple_fences = "Explanation:\n```markdown\n# Note\nThis is a preliminary thought\n```\nResult:\n```json\n{\"summary\": \"actual memory\"}\n```\nEnd.";
    assert_eq!(
        strip_json_fence(input_multiple_fences),
        "{\"summary\": \"actual memory\"}"
    );

    let input_with_unrelated_braces =
            "I considered {an invalid draft} first. Final: {\"summary\":\"usable\",\"events\":[]}. Done.";
    assert_eq!(
        strip_json_fence(input_with_unrelated_braces),
        "{\"summary\":\"usable\",\"events\":[]}"
    );

    let input_starting_with_unrelated_braces =
        "{an invalid draft} first. Final: {\"summary\":\"usable\",\"events\":[]}. Done.";
    assert_eq!(
        strip_json_fence(input_starting_with_unrelated_braces),
        "{\"summary\":\"usable\",\"events\":[]}"
    );

    let input_with_valid_metadata =
        "Trace: {\"request_id\":\"r1\"}\nFinal: {\"summary\":\"usable\",\"events\":[]}";
    assert_eq!(
        strip_json_fence(input_with_valid_metadata),
        "{\"summary\":\"usable\",\"events\":[]}"
    );
}

#[test]
fn malformed_outer_json_is_not_replaced_by_a_nested_recent_event() {
    let input = r#"```json
{
  "summary": "整理2023年以来"双通道"定点药店名单",
  "goal": "完成"双通道"名单初步填报",
  "result": "已生成结果文件",
  "source_references": [{"reference_key": "ref-t1-u"}],
  "events": [{
    "category": "progress",
    "title": "执行填报",
    "summary": "已完成初步填报",
    "source_reference": "ref-t1-p1"
  }]
}
```"#;

    let extracted = strip_json_fence(input);

    assert!(
            extracted.contains("\"source_references\""),
            "the extractor must keep the top-level memory object instead of promoting a nested event: {extracted}"
        );
    assert!(extracted.contains("ref-t1-u"));

    let parsed = parse_session_memory_agent_output(input)
        .expect("the narrow quote defect should be repaired locally");
    assert_eq!(parsed.summary, "整理2023年以来\"双通道\"定点药店名单");
    assert_eq!(parsed.source_references.len(), 1);
    assert_eq!(parsed.source_references[0].reference_key, "ref-t1-u");
    assert_eq!(parsed.events.len(), 1);
    assert_eq!(
        parsed.events[0].source_reference.as_deref(),
        Some("ref-t1-p1")
    );
}

#[test]
fn session_memory_output_validation_records_type_and_schema_errors() {
    let malformed_type_json = r#"{
            "summary": "ok",
            "goal": "test",
            "result": "test",
            "decisions": "should be an array of strings but got string",
            "verification": [],
            "blockers": [],
            "follow_up": [],
            "topics": [],
            "source_references": [],
            "events": []
        }"#;

    let parsed: Result<SessionMemoryAgentOutput, _> = serde_json::from_str(malformed_type_json);
    assert!(parsed.is_err());
    let err = parsed.unwrap_err();
    assert_eq!(err.classify(), serde_json::error::Category::Data);
    assert!(err.line() > 0);
}

#[tokio::test]
async fn session_memory_circuit_breaker_and_budget_protect_from_storm() {
    let root = std::env::temp_dir().join(format!("assetiweave-storm-protect-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create test root");
    let db_path = root.join("app.db");
    let fake = FakeRuntime::new();
    let service = AppService::open_with_db_path_and_runtime(db_path.clone(), fake.clone())
        .await
        .expect("open app service");

    let mut settings = service.app_settings_value();
    settings["memory"]["generationEnabled"] = serde_json::json!(true);
    service.runtime.update_app_settings_value(settings);

    let pool = service.db.pool();
    let now = Utc::now();
    let now_str = now.to_rfc3339();

    // 插入 3 个具有相同 last_error 的失败任务
    for i in 0..3 {
        let job_id = format!("failed-job-{}", i);
        let sess_id = format!("sess-fail-{}", i);
        sqlx::query(
            r#"
                INSERT INTO session_memory_jobs (
                    tenant_id, id, session_id, source_id, source_event_id, source_sync_run_id,
                    source_revision, source_fingerprint, contract_version, prompt_version,
                    status, not_before, attempt_count, retry_count,
                    last_error, created_at, updated_at
                ) VALUES (
                    'default', ?1, ?2, 'src-fail', 'evt-fail', 'sync-fail',
                    1, 'fp', 'v1', 'v1',
                    'failed', ?3, 1, 1,
                    'agent_execution_failed', ?3, ?3
                )
                "#,
        )
        .bind(&job_id)
        .bind(&sess_id)
        .bind(&now_str)
        .execute(pool)
        .await
        .unwrap();
    }

    // 调用调度对账器，应触发同错误熔断，直接返回 0
    let scheduled = service
        .reconcile_session_memory_jobs_for_tenant_at("default", now)
        .await
        .expect("reconcile call");
    assert_eq!(
        scheduled, 0,
        "Circuit breaker should prevent dispatch when 3 consecutive identical failures exist"
    );

    let _ = std::fs::remove_dir_all(root);
}
