use super::*;
use crate::backend::application::AppService;
use crate::backend::infrastructure::tasks::stages;
use chrono::Utc;

#[tokio::test]
async fn test_start_memory_pipeline_creates_single_task_with_three_stages() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-1-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();

    let handle = service
        .start_memory_pipeline(MemoryPipelineParams::background())
        .await
        .unwrap();

    assert_eq!(
        handle.task_id(),
        format!("memory-pipeline:{}", service.tenant_id())
    );

    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("pipeline snapshot");

    assert_eq!(snapshot.stages.len(), 3);
    assert_eq!(snapshot.stages[0].id, stages::SESSION_FACTS);
    assert_eq!(snapshot.stages[1].id, stages::RECENT_SNAPSHOT);
    assert_eq!(snapshot.stages[2].id, stages::GLOBAL_CONSOLIDATION);
    assert_eq!(
        snapshot.state,
        crate::backend::infrastructure::tasks::TaskState::Succeeded
    );
}

#[tokio::test]
async fn test_concurrency_rejection_returns_conflict() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-2-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();

    let task_id = format!("memory-pipeline:{}", service.tenant_id());
    // 手动注册一个活跃任务模拟并发
    let spec = crate::backend::infrastructure::tasks::TaskSpec::new(
        crate::backend::infrastructure::tasks::TaskKind::Memory,
        Some(task_id.clone()),
    )
    .with_task_id(task_id.clone())
    .with_tenant_id(service.tenant_id().to_string())
    .with_conflict_key(format!("memory:{}", service.tenant_id()))
    .with_conflict_policy(crate::backend::infrastructure::tasks::ConflictPolicy::Reject);

    let _h = service
        .runtime
        .task_runtime()
        .run(spec, move |_cx| async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            Ok(crate::backend::infrastructure::tasks::TaskOutput::new(
                serde_json::json!({}),
            ))
        })
        .unwrap();

    assert!(service.is_memory_pipeline_active());

    // 第二次调用必须返回 Conflict
    let conflict_res = service
        .start_memory_pipeline(MemoryPipelineParams::background())
        .await;

    assert!(conflict_res.is_err());
    assert!(matches!(conflict_res.unwrap_err(), AppError::Conflict(_)));
}

#[tokio::test]
async fn test_has_pending_memory_work_returns_false_when_clean() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-5-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();

    let now = Utc::now();
    let has_work = service.has_pending_memory_work(now).await.unwrap();
    assert!(
        !has_work,
        "clean database must NOT have pending memory work"
    );
}

#[tokio::test]
async fn test_pipeline_records_steps_for_all_executed_stages() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-6-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();

    let handle = service
        .start_memory_pipeline(MemoryPipelineParams::background())
        .await
        .unwrap();

    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("pipeline snapshot");

    // 阶段 1: SESSION_FACTS
    let stage1 = snapshot
        .stages
        .iter()
        .find(|s| s.id == stages::SESSION_FACTS)
        .expect("stage 1");
    assert_eq!(stage1.status, StageStatus::Succeeded);
    assert!(!stage1.steps.is_empty(), "stage 1 must record steps");

    // 阶段 2: RECENT_SNAPSHOT
    let stage2 = snapshot
        .stages
        .iter()
        .find(|s| s.id == stages::RECENT_SNAPSHOT)
        .expect("stage 2");
    assert_eq!(stage2.status, StageStatus::Succeeded);
    assert!(!stage2.steps.is_empty(), "stage 2 must record steps");

    // 阶段 3: GLOBAL_CONSOLIDATION
    let stage3 = snapshot
        .stages
        .iter()
        .find(|s| s.id == stages::GLOBAL_CONSOLIDATION)
        .expect("stage 3");
    assert_eq!(stage3.status, StageStatus::Succeeded);
    assert!(!stage3.steps.is_empty(), "stage 3 must record steps");
}

fn cumulative_node(
    id: &str,
    status: crate::backend::domain::memory::NodeStatus,
    summary: &str,
    parent_id: Option<&str>,
    turn_index: u32,
) -> crate::backend::domain::memory::MemoryTreeNode {
    crate::backend::domain::memory::MemoryTreeNode {
        id: id.to_string(),
        parent_id: parent_id.map(str::to_string),
        topic_id: "memory-integrity".to_string(),
        summary: summary.to_string(),
        status,
        source_session_id: "sess-cumulative".to_string(),
        turn_index,
        turn_started_at: Some("2026-10-20T10:00:00Z".to_string()),
        created_at: "2026-10-20T10:00:00Z".to_string(),
        updated_at: "2026-10-20T10:00:00Z".to_string(),
    }
}

#[tokio::test]
async fn test_force_rebuild_preserves_cumulative_memory_state() {
    use crate::backend::domain::memory::{NodeStatus, TopicEvolutionCommitPayload};

    let tmp = std::env::temp_dir().join(format!(
        "memory-pipeline-test-rebuild-{}",
        uuid::Uuid::new_v4()
    ));
    let db_path = tmp.join("db").join("app.db");
    std::fs::create_dir_all(tmp.join("db")).unwrap();
    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let pool = service.db.pool();
    let tenant_id = service.tenant_id().to_string();
    let storage = service.memory_storage();

    // 0. 哨兵：旧版默认记忆根目录（<tmp>/memories）里的资产绝不能被触碰
    let fallback_root = tmp.join("memories").join("default");
    std::fs::create_dir_all(fallback_root.join(".state/projects")).unwrap();
    let sentinel_state = fallback_root.join(".state/projects/sentinel-proj.json");
    std::fs::write(&sentinel_state, "{\"sentinel\":true}").unwrap();
    let sentinel_md = fallback_root.join("MEMORY.md");
    std::fs::write(&sentinel_md, "# 旧默认根目录全局记忆哨兵").unwrap();

    // 1. 连续三次提交，覆盖 completed / in-progress(continued) / unexplored 三种分支。
    //    三个节点都作为独立根节点挂载，父节点状态不会因下游演进被改写，
    //    这样断言才只反映「重建是否保留」，而不是提交语义。
    let commit =
        |node: crate::backend::domain::memory::MemoryTreeNode| TopicEvolutionCommitPayload {
            project_slug: "cumulative-proj".to_string(),
            topic_id: "memory-integrity".to_string(),
            topic_name: Some("记忆完整性".to_string()),
            parent_node_id: None,
            parent_new_status: None,
            new_node: node,
            unexplored_siblings: Vec::new(),
        };

    service
        .commit_memory_topic_evolution_to(
            commit(cumulative_node(
                "deadbeef#T1",
                NodeStatus::Completed,
                "已闭环的历史结论",
                None,
                1,
            )),
            &storage,
        )
        .await
        .expect("commit completed node");
    service
        .commit_memory_topic_evolution_to(
            commit(cumulative_node(
                "deadbeef#T2",
                NodeStatus::Continued,
                "仍在推进中的分支",
                None,
                2,
            )),
            &storage,
        )
        .await
        .expect("commit in-progress node");
    let mut unexplored_payload = commit(cumulative_node(
        "deadbeef#T3",
        NodeStatus::Completed,
        "第三轮闭环结论",
        None,
        3,
    ));
    unexplored_payload.unexplored_siblings = vec![cumulative_node(
        "deadbeef#T3.1",
        NodeStatus::Unexplored,
        "尚未闭环的悬空分支",
        None,
        3,
    )];
    service
        .commit_memory_topic_evolution_to(unexplored_payload, &storage)
        .await
        .expect("commit unexplored sibling");
    storage
        .write_markdown_atomic("sessions/legacy-sess.md", "# Legacy session")
        .unwrap();

    let fake_rec = store::MemorySessionWatermarkRecord {
        tenant_id: tenant_id.clone(),
        session_id: "fake-sess-1".to_string(),
        project_path: None,
        source_updated_at: "2026-10-26T12:00:00Z".to_string(),
        summary_rel_path: "sessions/fake-sess-1.md".to_string(),
        has_signal: true,
        updated_at: "2026-10-26T12:00:00Z".to_string(),
    };
    store::upsert_watermark_sqlx(pool, &fake_rec).await.unwrap();

    // 2. 带 force_rebuild 启动流水线
    let handle = service
        .start_memory_pipeline(MemoryPipelineParams::manual(true))
        .await
        .unwrap();
    let _ = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("pipeline snapshot");

    // 3. 累计真源不得被清空：三种状态的分支与水位全部保留
    let state = storage
        .read_project_state("cumulative-proj")
        .expect("read project state")
        .expect("cumulative state must survive force rebuild");
    let tree = state
        .topic_trees
        .get("memory-integrity")
        .expect("topic tree must survive force rebuild");
    assert_eq!(
        tree.nodes.get("deadbeef#T1").map(|n| n.status),
        Some(NodeStatus::Completed)
    );
    assert_eq!(
        tree.nodes.get("deadbeef#T2").map(|n| n.status),
        Some(NodeStatus::Continued),
        "进行中分支（continued）不得在重建中丢失"
    );
    assert_eq!(
        tree.nodes.get("deadbeef#T3.1").map(|n| n.status),
        Some(NodeStatus::Unexplored),
        "悬空叶子（unexplored）不得在重建中丢失"
    );
    assert_eq!(state.get_watermark("sess-cumulative"), Some(3));

    // 4. 水位表同样不得被清空
    let watermarks_after = store::load_watermarks_sqlx(pool, &tenant_id).await.unwrap();
    assert!(watermarks_after.contains_key("fake-sess-1"));

    // 5. 派生视图被重新投影，且包含悬空分支内容
    let project_md = storage
        .read_markdown("projects/cumulative-proj.md")
        .unwrap()
        .expect("project markdown regenerated");
    assert!(
        project_md.contains("尚未闭环的悬空分支"),
        "项目 Markdown 必须包含 unexplored 悬空分支"
    );
    let recent_md = storage
        .read_markdown("recent/latest.md")
        .unwrap()
        .expect("recent markdown regenerated");
    assert!(
        recent_md.contains("尚未闭环的悬空分支"),
        "近期工作 Markdown 必须跨窗口保留 unexplored 悬空分支"
    );

    // 6. 旧默认根目录哨兵资产毫发无损
    assert!(sentinel_state.exists());
    assert!(sentinel_md.exists());

    std::fs::remove_dir_all(tmp).ok();
}

#[tokio::test]
async fn test_unattended_pipeline_does_not_write_junk_to_disk() {
    let tmp = std::env::temp_dir().join(format!("memory-pipeline-nojunk-{}", uuid::Uuid::new_v4()));
    let db_path = tmp.join("db").join("app.db");
    std::fs::create_dir_all(tmp.join("db")).unwrap();
    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let storage = service.memory_storage();

    // 运行无人值守流水线
    let handle = service
        .start_memory_pipeline(MemoryPipelineParams::background())
        .await
        .unwrap();

    let _ = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("pipeline snapshot");

    let sessions_dir = storage.sessions_dir();
    if sessions_dir.exists() {
        let entries: Vec<_> = std::fs::read_dir(&sessions_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(
            entries.len(),
            0,
            "Unattended pipeline must NOT write sessions/*.md"
        );
    }

    assert!(
        !storage.recent_dir().join("latest.md").exists(),
        "Unattended pipeline must NOT write recent/latest.md"
    );
    assert!(
        !storage.global_file().exists(),
        "Unattended pipeline must NOT write MEMORY.md"
    );

    std::fs::remove_dir_all(tmp).ok();
}

#[tokio::test]
async fn test_has_pending_memory_work_respects_window_and_internal_filter() {
    let db_path = std::env::temp_dir().join(format!(
        "memory-pipeline-test-pending-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let pool = service.db.pool();
    let tenant_id = service.tenant_id();

    let adapter = crate::backend::domain::ConversationAdapter {
        id: "adapter-pending".to_string(),
        name: "Test Adapter".to_string(),
        kind: crate::backend::domain::ConversationAdapterKind::External,
        version: "1.0.0".to_string(),
        enabled: true,
        manifest_path: None,
        executable_path: None,
        content_hash: None,
        trusted_hash: None,
        trust_state: crate::backend::domain::ConversationAdapterTrustState::Trusted,
        protocol_version: Some(1),
        capabilities: vec!["read_session".to_string()],
        input_kinds: vec![crate::backend::domain::ConversationSourceKind::Directory],
        card_contract_version: None,
        card_kinds: Vec::new(),
        projection_version: Some(1),
        created_at: "2026-10-21T10:00:00Z".to_string(),
        updated_at: "2026-10-21T10:00:00Z".to_string(),
    };
    let source = crate::backend::domain::ConversationSource {
        id: "source-pending".to_string(),
        adapter_id: "adapter-pending".to_string(),
        name: "Test Source".to_string(),
        kind: crate::backend::domain::ConversationSourceKind::Directory,
        location: "/fixture/path".to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: "2026-10-21T10:00:00Z".to_string(),
        updated_at: "2026-10-21T10:00:00Z".to_string(),
    };

    crate::backend::store::upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter)
        .await
        .unwrap();
    crate::backend::store::upsert_conversation_source_sqlx(pool, tenant_id, &source)
        .await
        .unwrap();

    let now = chrono::Utc::now();
    // 1. Expired session (100h ago, outside 48h/72h window)
    let ts_expired = (now - chrono::Duration::hours(100)).to_rfc3339();
    let expired_session = crate::backend::domain::NormalizedConversationSession {
        external_id: "s-pending-expired".to_string(),
        title: Some("Expired".to_string()),
        project_path: Some("/code/expired".to_string()),
        started_at: Some(ts_expired.clone()),
        updated_at: Some(ts_expired.clone()),
        source_locator: None,
        source_fingerprint: None,
        turns: vec![crate::backend::domain::NormalizedConversationTurn {
            external_id: Some("t1".to_string()),
            turn_index: 0,
            user_text: "old work".to_string(),
            title: None,
            started_at: Some(ts_expired.clone()),
            ended_at: Some(ts_expired.clone()),
            model: None,
            parts: vec![],
            ..Default::default()
        }],
        execution_origin: None,
        execution_purpose: None,
        user_visible: Some(true),
    };

    // 2. Internal agent session
    let internal_root =
        crate::backend::infrastructure::agent_execution::agent_execution_workspace_root(
            &service.db_path,
        );
    let internal_session = crate::backend::domain::NormalizedConversationSession {
        external_id: "s-pending-internal".to_string(),
        title: Some("Internal".to_string()),
        project_path: Some(format!("{internal_root}/agent-run-1")),
        started_at: Some((now - chrono::Duration::hours(1)).to_rfc3339()),
        updated_at: Some((now - chrono::Duration::hours(1)).to_rfc3339()),
        source_locator: None,
        source_fingerprint: None,
        turns: vec![crate::backend::domain::NormalizedConversationTurn {
            external_id: Some("t2".to_string()),
            turn_index: 0,
            user_text: "internal run".to_string(),
            title: None,
            started_at: Some((now - chrono::Duration::hours(1)).to_rfc3339()),
            ended_at: Some((now - chrono::Duration::hours(1)).to_rfc3339()),
            model: None,
            parts: vec![],
            ..Default::default()
        }],
        execution_origin: Some("internal_agent".to_string()),
        execution_purpose: None,
        user_visible: Some(false),
    };

    crate::backend::store::import_conversation_sessions_sqlx(
        pool,
        tenant_id,
        &source,
        &[expired_session, internal_session],
        false,
    )
    .await
    .unwrap();

    // Expired and internal sessions must be ignored -> has_pending_memory_work is FALSE
    assert!(
        !service.has_pending_memory_work(now).await.unwrap(),
        "Expired and internal sessions must not trigger pending memory work"
    );

    // 3. Add an active session within recent window
    let ts_active = (now - chrono::Duration::hours(2)).to_rfc3339();
    let active_session = crate::backend::domain::NormalizedConversationSession {
        external_id: "s-pending-active".to_string(),
        title: Some("Active".to_string()),
        project_path: Some("/code/active".to_string()),
        started_at: Some(ts_active.clone()),
        updated_at: Some(ts_active.clone()),
        source_locator: None,
        source_fingerprint: None,
        turns: vec![crate::backend::domain::NormalizedConversationTurn {
            external_id: Some("t3".to_string()),
            turn_index: 0,
            user_text: "active recent work".to_string(),
            title: None,
            started_at: Some(ts_active.clone()),
            ended_at: Some(ts_active.clone()),
            model: None,
            parts: vec![],
            ..Default::default()
        }],
        execution_origin: None,
        execution_purpose: None,
        user_visible: Some(true),
    };

    crate::backend::store::import_conversation_sessions_sqlx(
        pool,
        tenant_id,
        &source,
        &[active_session],
        false,
    )
    .await
    .unwrap();

    // Now has_pending_memory_work should be TRUE
    assert!(
        service.has_pending_memory_work(now).await.unwrap(),
        "Active session within recent window must trigger pending memory work"
    );
}

/// 第一阶段会话发现必须能回溯到早已滑出近期窗口、但水位仍为 0 的积压会话。
#[tokio::test]
async fn test_pipeline_session_discovery_backfills_backlog_outside_recent_window() {
    use crate::backend::application::memory::recent::{
        RecentConversationSessionListParams, RecentConversationView,
    };

    let tmp =
        std::env::temp_dir().join(format!("memory-pipeline-backlog-{}", uuid::Uuid::new_v4()));
    let db_path = tmp.join("db").join("app.db");
    std::fs::create_dir_all(tmp.join("db")).unwrap();
    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let pool = service.db.pool();
    let tenant_id = service.tenant_id().to_string();
    let storage = service.memory_storage();

    let adapter = crate::backend::domain::ConversationAdapter {
        id: "adapter-backlog".to_string(),
        name: "Backlog Adapter".to_string(),
        kind: crate::backend::domain::ConversationAdapterKind::External,
        version: "1.0.0".to_string(),
        enabled: true,
        manifest_path: None,
        executable_path: None,
        content_hash: None,
        trusted_hash: None,
        trust_state: crate::backend::domain::ConversationAdapterTrustState::Trusted,
        protocol_version: Some(1),
        capabilities: vec!["read_session".to_string()],
        input_kinds: vec![crate::backend::domain::ConversationSourceKind::Directory],
        card_contract_version: None,
        card_kinds: Vec::new(),
        projection_version: Some(1),
        created_at: "2026-10-21T10:00:00Z".to_string(),
        updated_at: "2026-10-21T10:00:00Z".to_string(),
    };
    let source = crate::backend::domain::ConversationSource {
        id: "source-backlog".to_string(),
        adapter_id: "adapter-backlog".to_string(),
        name: "Backlog Source".to_string(),
        kind: crate::backend::domain::ConversationSourceKind::Directory,
        location: "/fixture/path".to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: "2026-10-21T10:00:00Z".to_string(),
        updated_at: "2026-10-21T10:00:00Z".to_string(),
    };
    crate::backend::store::upsert_conversation_adapter_sqlx(pool, &tenant_id, &adapter)
        .await
        .unwrap();
    crate::backend::store::upsert_conversation_source_sqlx(pool, &tenant_id, &source)
        .await
        .unwrap();

    let now = chrono::Utc::now();
    let backlog_ts = (now - chrono::Duration::days(30)).to_rfc3339();
    let active_ts = (now - chrono::Duration::hours(2)).to_rfc3339();

    let build_turn =
        |index: i64, ts: &str, text: &str| crate::backend::domain::NormalizedConversationTurn {
            external_id: Some(format!("t{index}")),
            turn_index: index,
            user_text: text.to_string(),
            title: None,
            started_at: Some(ts.to_string()),
            ended_at: Some(ts.to_string()),
            model: None,
            parts: vec![],
            ..Default::default()
        };

    let backlog_session = crate::backend::domain::NormalizedConversationSession {
        external_id: "s-backlog-never-extracted".to_string(),
        title: Some("Backlog".to_string()),
        project_path: Some("~/code-space/backlog-project".to_string()),
        started_at: Some(backlog_ts.clone()),
        updated_at: Some(backlog_ts.clone()),
        source_locator: None,
        source_fingerprint: None,
        turns: vec![
            build_turn(0, &backlog_ts, "积压会话第一轮"),
            build_turn(1, &backlog_ts, "积压会话第二轮"),
        ],
        execution_origin: None,
        execution_purpose: None,
        user_visible: Some(true),
    };
    let active_session = crate::backend::domain::NormalizedConversationSession {
        external_id: "s-active-recent".to_string(),
        title: Some("Active".to_string()),
        project_path: Some("~/code-space/active-project".to_string()),
        started_at: Some(active_ts.clone()),
        updated_at: Some(active_ts.clone()),
        source_locator: None,
        source_fingerprint: None,
        turns: vec![build_turn(0, &active_ts, "近期会话")],
        execution_origin: None,
        execution_purpose: None,
        user_visible: Some(true),
    };

    crate::backend::store::import_conversation_sessions_sqlx(
        pool,
        &tenant_id,
        &source,
        &[backlog_session, active_session],
        false,
    )
    .await
    .unwrap();

    // 1. 默认近期窗口只会看到活跃会话，积压会话被窗口裁掉
    let windowed = service
        .list_recent_conversation_sessions_at(
            RecentConversationSessionListParams {
                view: RecentConversationView::Time,
                window_hours: None,
                limit: Some(100),
                offset: Some(0),
            },
            now,
        )
        .await
        .unwrap();
    let windowed_ext: Vec<_> = windowed
        .iter()
        .map(|s| s.session.external_id.clone())
        .collect();
    assert!(windowed_ext.contains(&"s-active-recent".to_string()));
    assert!(
        !windowed_ext.contains(&"s-backlog-never-extracted".to_string()),
        "默认近期窗口不应包含 30 天前的积压会话"
    );

    // 2. 不限窗口（回溯全部）时两个会话都必须被发现
    let backfilled = service
        .list_recent_conversation_sessions_at(
            RecentConversationSessionListParams {
                view: RecentConversationView::Time,
                window_hours: Some(0),
                limit: Some(100),
                offset: Some(0),
            },
            now,
        )
        .await
        .unwrap();
    let backfilled_ext: Vec<_> = backfilled
        .iter()
        .map(|s| s.session.external_id.clone())
        .collect();
    assert!(backfilled_ext.contains(&"s-active-recent".to_string()));
    assert!(
        backfilled_ext.contains(&"s-backlog-never-extracted".to_string()),
        "回溯扫描必须发现从未提炼（水位 0）的历史积压会话"
    );

    // 3. 积压会话确实还没有任何水位记录
    let backlog_item = backfilled
        .iter()
        .find(|s| s.session.external_id == "s-backlog-never-extracted")
        .expect("backlog session discovered");
    assert_eq!(backlog_item.turn_count, 2);
    let slug = crate::backend::application::memory::project_path_to_slug(
        backlog_item.project_path.as_deref().unwrap_or(""),
    );
    assert_eq!(
        service
            .check_memory_session_watermark_in(&slug, &backlog_item.session.id, &storage)
            .await
            .unwrap(),
        None
    );

    // 4. 流水线第一阶段必须按回溯扫描口径发现积压会话
    let report = service
        .run_session_branch_workflow_at(None, Some(&storage), now)
        .await
        .unwrap();
    assert_eq!(
        report.scanned_count, 2,
        "流水线扫描必须同时覆盖窗口内与窗口外的积压会话"
    );

    std::fs::remove_dir_all(tmp).ok();
}

/// 未处理会话发现必须在分页之前完成过滤：小 limit 不能把积压会话截断掉。
#[tokio::test]
async fn test_unprocessed_session_scan_filters_before_paging() {
    use crate::backend::application::memory::UnprocessedSessionScanParams;

    let tmp =
        std::env::temp_dir().join(format!("memory-unprocessed-scan-{}", uuid::Uuid::new_v4()));
    let db_path = tmp.join("db").join("app.db");
    std::fs::create_dir_all(tmp.join("db")).unwrap();
    let service = AppService::open_with_db_path(db_path).await.unwrap();
    let pool = service.db.pool();
    let tenant_id = service.tenant_id().to_string();
    let storage = service.memory_storage();

    let adapter = crate::backend::domain::ConversationAdapter {
        id: "adapter-scan".to_string(),
        name: "Scan Adapter".to_string(),
        kind: crate::backend::domain::ConversationAdapterKind::External,
        version: "1.0.0".to_string(),
        enabled: true,
        manifest_path: None,
        executable_path: None,
        content_hash: None,
        trusted_hash: None,
        trust_state: crate::backend::domain::ConversationAdapterTrustState::Trusted,
        protocol_version: Some(1),
        capabilities: vec!["read_session".to_string()],
        input_kinds: vec![crate::backend::domain::ConversationSourceKind::Directory],
        card_contract_version: None,
        card_kinds: Vec::new(),
        projection_version: Some(1),
        created_at: "2026-10-21T10:00:00Z".to_string(),
        updated_at: "2026-10-21T10:00:00Z".to_string(),
    };
    let source = crate::backend::domain::ConversationSource {
        id: "source-scan".to_string(),
        adapter_id: "adapter-scan".to_string(),
        name: "Scan Source".to_string(),
        kind: crate::backend::domain::ConversationSourceKind::Directory,
        location: "/fixture/scan".to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: "2026-10-21T10:00:00Z".to_string(),
        updated_at: "2026-10-21T10:00:00Z".to_string(),
    };
    crate::backend::store::upsert_conversation_adapter_sqlx(pool, &tenant_id, &adapter)
        .await
        .unwrap();
    crate::backend::store::upsert_conversation_source_sqlx(pool, &tenant_id, &source)
        .await
        .unwrap();

    let now = chrono::Utc::now();
    // 3 条「新近但已提炼完」的会话 + 1 条「最旧但从未提炼」的积压会话。
    // 积压会话按 updated_at 倒序排在最后，只有分页前过滤才能被检出。
    let mut sessions = Vec::new();
    for index in 0..3 {
        let ts = (now - chrono::Duration::minutes(index as i64 + 1)).to_rfc3339();
        sessions.push(crate::backend::domain::NormalizedConversationSession {
            external_id: format!("s-done-{index}"),
            title: Some(format!("Done {index}")),
            project_path: Some("~/code-space/scan-project".to_string()),
            started_at: Some(ts.clone()),
            updated_at: Some(ts.clone()),
            source_locator: None,
            source_fingerprint: None,
            turns: vec![crate::backend::domain::NormalizedConversationTurn {
                external_id: Some("t0".to_string()),
                turn_index: 0,
                user_text: "已提炼完成的会话".to_string(),
                title: None,
                started_at: Some(ts.clone()),
                ended_at: Some(ts),
                model: None,
                parts: vec![],
                ..Default::default()
            }],
            execution_origin: None,
            execution_purpose: None,
            user_visible: Some(true),
        });
    }
    let backlog_ts = (now - chrono::Duration::days(20)).to_rfc3339();
    sessions.push(crate::backend::domain::NormalizedConversationSession {
        external_id: "s-backlog".to_string(),
        title: Some("Backlog".to_string()),
        project_path: Some("~/code-space/scan-project".to_string()),
        started_at: Some(backlog_ts.clone()),
        updated_at: Some(backlog_ts.clone()),
        source_locator: None,
        source_fingerprint: None,
        turns: vec![crate::backend::domain::NormalizedConversationTurn {
            external_id: Some("t0".to_string()),
            turn_index: 0,
            user_text: "从未提炼的积压会话".to_string(),
            title: None,
            started_at: Some(backlog_ts.clone()),
            ended_at: Some(backlog_ts),
            model: None,
            parts: vec![],
            ..Default::default()
        }],
        execution_origin: None,
        execution_purpose: None,
        user_visible: Some(true),
    });

    crate::backend::store::import_conversation_sessions_sqlx(
        pool, &tenant_id, &source, &sessions, false,
    )
    .await
    .unwrap();

    // 把 3 条新近会话的水位推到满，只留下积压会话未处理
    let mut project_state = crate::backend::domain::memory::ProjectMemoryState::new(
        crate::backend::application::memory::project_path_to_slug("~/code-space/scan-project"),
    );
    let done_ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND external_id LIKE 's-done-%'",
    )
    .bind(&tenant_id)
    .fetch_all(pool)
    .await
    .unwrap();
    for id in &done_ids {
        project_state.set_watermark(id, 1);
    }
    storage
        .write_project_state_atomic(
            &crate::backend::application::memory::project_path_to_slug("~/code-space/scan-project"),
            &project_state,
        )
        .unwrap();

    let scanned = service
        .list_unprocessed_conversation_sessions(UnprocessedSessionScanParams {
            list: asset_core_params(),
            limit: 1,
            offset: 0,
        })
        .await
        .unwrap();
    assert_eq!(scanned.len(), 1);
    assert_eq!(scanned[0].session.external_id, "s-backlog");

    fn asset_core_params() -> crate::backend::application::ConversationSessionListParams {
        crate::backend::application::ConversationSessionListParams {
            limit: Some(1),
            offset: Some(0),
            ..Default::default()
        }
    }

    std::fs::remove_dir_all(tmp).ok();
}
