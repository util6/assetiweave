use super::*;
use crate::backend::dto::{RecentMemoryStatus, RecentSnapshotPublicationKind};
use crate::backend::models::{
    MemoryGenerationCoverageV2, MemoryGenerationItemV2, MemoryGenerationProjectV2,
    MemoryGenerationResultV2, MemoryItemCategory, MemoryItemStatus, MemoryPromotionNomination,
};
use chrono::TimeZone;
use std::fs;
use std::path::Path;

#[test]
fn memory_generation_output_accepts_only_one_json_value() {
    let valid = r#"{"schemaVersion":2,"projects":[],"coverage":{"coveredSessions":[],"noMemorySessions":[]},"unknowns":[]}"#;
    assert!(parse_memory_generation_output(valid).is_ok());
    assert!(parse_memory_generation_output(&format!("```json\n{valid}\n```")).is_ok());
    assert!(parse_memory_generation_output("{} trailing prose").is_err());
    assert!(parse_memory_generation_output("```json\n{}\n").is_err());
}

#[test]
fn memory_generation_output_repairs_truncated_json() {
    let truncated = r#"{"schemaVersion":2,"projects":[{"projectKey":"p1","summary":"P1 summary","sourceSessions":["s1"],"items":[{"title":"Item 1","category":"progress","status":"completed","summary":"ok","rationale":"ok","occurredAt":"2026-09-15T12:00:00Z","sourceRefs":["s1.r1"],"recommendationRank":1,"promotionNomination":"none"}],"noMaterialChange":false}],"coverage":{"coveredSessions":["s1"],"noMemorySessions":[]},"unknowns":[],"extra":"cut off here"#;
    match parse_memory_generation_output(truncated) {
        Ok(res) => assert_eq!(res.projects.len(), 1),
        Err(e) => panic!("parse_memory_generation_output failed: {e}"),
    }
}

#[test]
fn memory_generation_output_accepts_status_and_category_aliases() {
    let json_with_aliases = r#"{"schemaVersion":2,"projects":[{"projectKey":"p1","summary":"P1 summary","sourceSessions":["s1"],"items":[{"title":"Item 1","category":"todo","status":"working","summary":"ok","rationale":"ok","occurredAt":"2026-09-15T12:00:00Z","sourceRefs":["s1.r1"],"recommendationRank":1,"promotionNomination":"none"}],"noMaterialChange":false}],"coverage":{"coveredSessions":["s1"],"noMemorySessions":[]},"unknowns":[]}"#;
    let res = parse_memory_generation_output(json_with_aliases).unwrap();
    assert_eq!(
        res.projects[0].items[0].category,
        MemoryItemCategory::FollowUp
    );
    assert_eq!(res.projects[0].items[0].status, MemoryItemStatus::Active);
}

#[test]
fn recent_generation_prompt_embeds_schema_and_a_bounded_tool_contract() {
    let payload = RecentSnapshotWorkOrderPayload {
        target_watermark_utc: "2026-09-15T12:00:00Z".to_string(),
        local_watermark_date: "2026-09-15".to_string(),
        local_watermark_time: "14:00".to_string(),
        timezone_offset_minutes: 120,
        window_hours: 48,
        window_start_utc: "2026-09-13T12:00:00Z".to_string(),
        window_end_utc: "2026-09-15T12:00:00Z".to_string(),
        target_fingerprint: "target-fingerprint".to_string(),
        content_fingerprint: "content-fingerprint".to_string(),
        skill: MemorySkillBinding {
            asset_id: "skill-fixture".to_string(),
            asset_revision: 1,
            content_hash: "skill-hash".to_string(),
            entry_hash: "entry-hash".to_string(),
        },
        skill_text: "Generate recent memory.".to_string(),
        evidence: RecentSnapshotWorkOrderEvidencePack {
            target_watermark_utc: "2026-09-15T12:00:00Z".to_string(),
            window_start_utc: "2026-09-13T12:00:00Z".to_string(),
            window_end_utc: "2026-09-15T12:00:00Z".to_string(),
            window_hours: 48,
            project_keys: Vec::new(),
            candidate_sessions: Vec::new(),
            session_evidence: vec![RecentSnapshotSessionEvidence {
                candidate: CandidateSessionSummary {
                    session_ref: "s1".to_string(),
                    session_id: "session-internal-id".to_string(),
                    project_key: "project-fixture".to_string(),
                    title: "Fixture".to_string(),
                    last_activity_at: "2026-09-15T11:00:00Z".to_string(),
                    source_id: "source-internal-id".to_string(),
                    source_agent: "codex".to_string(),
                    source_revision: 1,
                },
                memory_source_revision: Some(1),
                summary: Some("Frozen summary".to_string()),
                goal: None,
                result: None,
                decisions: Vec::new(),
                verification: Vec::new(),
                blockers: Vec::new(),
                follow_up: Vec::new(),
                topics: Vec::new(),
                source_references: vec![SessionMemorySourceReference {
                    tenant_id: "default".to_string(),
                    id: "source-reference-row-id".to_string(),
                    memory_id: "memory-internal-id".to_string(),
                    source_id: "source-internal-id".to_string(),
                    session_id: "session-internal-id".to_string(),
                    question_id: Some("question-internal-id".to_string()),
                    turn_id: Some("turn-internal-id".to_string()),
                    part_id: None,
                    node_id: None,
                    node_order: None,
                    reference_key: "session-memory-ref-persistent-secret".to_string(),
                    source_revision: 1,
                    created_at: "2026-09-15T11:00:00Z".to_string(),
                }],
                recent_events: Vec::new(),
            }],
            continuable_items: vec![ContinuableMemoryItemView {
                item_id: "item-fixture".to_string(),
                project_key: "project-fixture".to_string(),
                category: MemoryItemCategory::Progress,
                status: MemoryItemStatus::Active,
                title: "Continue fixture".to_string(),
                summary: "Summary".to_string(),
                rationale: "Rationale".to_string(),
                first_seen_at: "2026-09-15T10:00:00Z".to_string(),
                last_seen_at: "2026-09-15T11:00:00Z".to_string(),
                days_since_first_seen: 0,
                remaining_days: 7,
                current_revision_id: "revision-fixture".to_string(),
                current_revision_number: 1,
                evidence_fingerprint: "evidence-fixture".to_string(),
                source_refs: vec!["session-memory-ref-persistent-secret".to_string()],
            }],
            current_l2_projects: Vec::new(),
            current_l3_items: Vec::new(),
            allowed_tools: ALLOWED_MEMORY_GENERATION_TOOLS
                .iter()
                .map(|tool| (*tool).to_string())
                .collect(),
            output_schema_version: 2,
        },
    };
    let envelope = serde_json::json!({
        "workOrder": { "contractVersion": "memory.contract.v2" }
    });

    let prompt = build_recent_generation_prompt(&envelope, &payload).unwrap();
    let prompt: serde_json::Value = serde_json::from_str(&prompt).unwrap();

    assert_eq!(prompt["contract"], "memory.contract.v2");
    assert_eq!(
        prompt["execution_policy"]["allowed_tools"],
        serde_json::json!(ALLOWED_MEMORY_GENERATION_TOOLS)
    );
    assert!(prompt["output_schema"]["properties"]["schemaVersion"].is_object());
    assert!(prompt["instruction"]
        .as_str()
        .unwrap()
        .contains("Never inspect the workspace"));
    assert!(prompt["instruction"]
        .as_str()
        .unwrap()
        .contains("sourceRefs"));
    assert_eq!(
        prompt["evidence"]["sessionEvidence"][0]["sourceReferences"][0]["reference_key"],
        "s1.r1"
    );
    assert!(!prompt
        .to_string()
        .contains("session-memory-ref-persistent-secret"));
}

#[test]
fn frozen_source_reference_aliases_resolve_to_the_persistent_locator() {
    let candidate = CandidateSession {
        tenant_id: "default".to_string(),
        session_id: "session-1".to_string(),
        source_id: "source-1".to_string(),
        session_title: "Fixture".to_string(),
        source_agent: "codex".to_string(),
        project_key: "project-1".to_string(),
        project_path: Some("project-1".to_string()),
        last_activity_at: "2026-09-15T11:00:00Z".to_string(),
        source_revision: 3,
        short_ref: "s1".to_string(),
    };
    let reference = SessionMemorySourceReference {
        tenant_id: "default".to_string(),
        id: "source-reference-row-id".to_string(),
        memory_id: "memory-1".to_string(),
        source_id: "source-1".to_string(),
        session_id: "session-1".to_string(),
        question_id: Some("question-1".to_string()),
        turn_id: Some("turn-1".to_string()),
        part_id: None,
        node_id: None,
        node_order: None,
        reference_key: "session-memory-ref-persistent".to_string(),
        source_revision: 3,
        created_at: "2026-09-15T11:00:00Z".to_string(),
    };
    let mut ref_map = HashMap::new();

    extend_source_reference_aliases(&mut ref_map, &candidate, std::slice::from_ref(&reference));

    let resolved = ref_map.get("s1.r1").expect("short ref alias");
    assert_eq!(resolved.reference_key, reference.reference_key);
    assert_eq!(resolved.question_id.as_deref(), Some("question-1"));
    assert!(ref_map.contains_key("session-memory-ref-persistent"));
}

#[test]
fn agent_result_normalization_drops_unassigned_promotion_nomination() {
    let mut result = MemoryGenerationResultV2 {
        schema_version: 2,
        projects: vec![MemoryGenerationProjectV2 {
            project_key: "unassigned".to_string(),
            summary: "Summary".to_string(),
            no_material_change: false,
            source_sessions: vec!["s1".to_string()],
            items: vec![MemoryGenerationItemV2 {
                continues_item_id: None,
                category: MemoryItemCategory::Research,
                status: MemoryItemStatus::Verified,
                title: "Finding".to_string(),
                summary: "Summary".to_string(),
                rationale: "Rationale".to_string(),
                occurred_at: "2026-09-15T11:00:00Z".to_string(),
                recommendation_rank: None,
                source_refs: vec!["s1.r1".to_string()],
                promotion_nomination: MemoryPromotionNomination::ResearchConclusion,
            }],
        }],
        coverage: MemoryGenerationCoverageV2::default(),
        unknowns: Vec::new(),
    };

    normalize_agent_memory_generation_result(&mut result, &[], &HashMap::new());

    assert_eq!(
        result.projects[0].items[0].promotion_nomination,
        MemoryPromotionNomination::None
    );
}

#[test]
fn recent_generation_tools_are_bound_to_the_running_job_lease() {
    let mut job = recent_memory_job_fixture();
    job.ownership_token = Some("lease-fixture".to_string());

    let tools = memory_generation_tools_for_job(&job, Path::new("/tmp/fixture.db")).unwrap();

    assert_eq!(tools.tenant_id, "default");
    assert_eq!(tools.job_id, "recent-job-fixture");
    assert_eq!(tools.ownership_token, "lease-fixture");
    assert_eq!(tools.database_path, "/tmp/fixture.db");

    job.ownership_token = None;
    assert!(memory_generation_tools_for_job(&job, Path::new("/tmp/fixture.db")).is_err());
}

fn recent_memory_job_fixture() -> store::RecentMemoryJob {
    store::RecentMemoryJob {
        tenant_id: "default".to_string(),
        id: "recent-job-fixture".to_string(),
        status: "running".to_string(),
        ownership_token: None,
        lease_expires_at: None,
        heartbeat_at: None,
        attempt_count: 1,
        retry_count: 0,
        retry_at: None,
        target_watermark_utc: "2026-09-15T12:00:00Z".to_string(),
        window_hours: 48,
        target_fingerprint: "target-fingerprint".to_string(),
        content_fingerprint: "content-fingerprint".to_string(),
        work_order_json: "{}".to_string(),
        last_error_code: None,
        last_error_message: None,
        started_at: None,
        finished_at: None,
        created_at: "2026-09-15T12:00:00Z".to_string(),
        updated_at: "2026-09-15T12:00:00Z".to_string(),
    }
}

async fn setup_test_service() -> (AppService, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-snapshot-pipeline-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let db_path = root.join("app.db");

    let service = AppService::open_with_db_path(db_path)
        .await
        .expect("open service");

    (service, root)
}

async fn seed_test_conversation_data(service: &AppService) {
    let pool = service.db.pool();

    // 1. Insert conversation source
    sqlx::query(
        r#"
            INSERT INTO conversation_sources (
                tenant_id, id, adapter_id, name, kind, location, config_json, enabled,
                last_synced_at, last_sync_status, created_at, updated_at
            ) VALUES (
                'default', 'source-alpha', 'adapter-claude', 'Alpha Source', 'local_folder',
                '/tmp/source', '{}', 1, '2026-09-14T00:00:00Z', 'idle',
                '2026-09-14T00:00:00Z', '2026-09-14T00:00:00Z'
            )
            "#,
    )
    .execute(pool)
    .await
    .expect("insert source");

    // Target watermark: 2026-09-15T12:00:00Z, 48h cutoff: 2026-09-13T12:00:00Z

    // Session 1: in window (10h before watermark), project alpha
    insert_test_session(
        pool,
        "session-in-alpha",
        "source-alpha",
        "Alpha In-Window",
        Some("/tmp/alpha-project"),
        "2026-09-15T02:00:00Z",
    )
    .await;

    // Session 2: in window (30h before watermark), unassigned (no project)
    insert_test_session(
        pool,
        "session-in-unassigned",
        "source-alpha",
        "Unassigned In-Window",
        None,
        "2026-09-14T06:00:00Z",
    )
    .await;

    // Session 3: too old (outside 48h window: 50h before watermark)
    insert_test_session(
        pool,
        "session-too-old",
        "source-alpha",
        "Too Old Session",
        Some("/tmp/alpha-project"),
        "2026-09-13T10:00:00Z",
    )
    .await;

    // Session 4: in future (after watermark)
    insert_test_session(
        pool,
        "session-in-future",
        "source-alpha",
        "Future Session",
        Some("/tmp/alpha-project"),
        "2026-09-15T13:00:00Z",
    )
    .await;
}

async fn insert_test_session(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    source_id: &str,
    title: &str,
    project_path: Option<&str>,
    activity_at: &str,
) {
    sqlx::query(
        r#"
            INSERT INTO conversation_sessions (
                tenant_id, id, source_id, adapter_id, external_id, title,
                project_path, started_at, updated_at, source_locator,
                source_fingerprint, missing, user_visible, created_at, imported_at
            ) VALUES (
                'default', ?1, ?2, 'adapter-claude', ?1, ?3, ?4,
                ?5, ?5, 'loc', 'fp', 0, 1, ?5, ?5
            )
            "#,
    )
    .bind(session_id)
    .bind(source_id)
    .bind(title)
    .bind(project_path)
    .bind(activity_at)
    .execute(pool)
    .await
    .expect("insert test session");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_recent_snapshot_pipeline_candidate_selection() {
    let (service, _root) = setup_test_service().await;
    seed_test_conversation_data(&service).await;

    // Session 5: in window (12h before watermark), but will be excluded via settings
    insert_test_session(
        service.db.pool(),
        "session-excluded",
        "source-alpha",
        "Excluded Session",
        Some("/tmp/alpha-project"),
        "2026-09-15T00:00:00Z",
    )
    .await;

    // Configure exclusion in settings
    service
        .save_app_settings(serde_json::json!({
            "memory": {
                "generationEnabled": true,
                "usageEnabled": true,
                "recentWindowHours": 48,
                "watermarkTime1": "02:00",
                "watermarkTime2": "14:00",
                "generationSkillAssetId": null,
                "excludedSessionIds": ["session-excluded"],
                "excludedSourceIds": []
            }
        }))
        .await
        .expect("save settings");

    let target_watermark: DateTime<Utc> = "2026-09-15T12:00:00Z".parse().unwrap();
    let (candidates, ref_map) = service
        .collect_recent_snapshot_candidates(target_watermark, 48)
        .await
        .expect("collect candidates");

    // Exactly 2 candidates: session-in-alpha and session-in-unassigned
    assert_eq!(candidates.len(), 2);

    let alpha_candidate = candidates
        .iter()
        .find(|c| c.session_id == "session-in-alpha")
        .expect("find alpha candidate");
    let expected_alpha_key = resolve_project_directory("/tmp/alpha-project", &[])
        .unwrap_or_else(|| "/tmp/alpha-project".to_string());
    assert_eq!(alpha_candidate.project_key, expected_alpha_key);
    assert_eq!(
        alpha_candidate.project_path.as_deref(),
        Some(expected_alpha_key.as_str())
    );
    assert!(!alpha_candidate.short_ref.is_empty());

    let unassigned_candidate = candidates
        .iter()
        .find(|c| c.session_id == "session-in-unassigned")
        .expect("find unassigned candidate");
    assert_eq!(unassigned_candidate.project_key, "unassigned");
    assert_eq!(unassigned_candidate.project_path, None);
    assert!(!unassigned_candidate.short_ref.is_empty());

    // Verify ref_map has both short refs
    assert!(ref_map.contains_key(&alpha_candidate.short_ref));
    assert!(ref_map.contains_key(&unassigned_candidate.short_ref));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_recent_snapshot_pipeline_quality_gates() {
    let (service, _root) = setup_test_service().await;
    seed_test_conversation_data(&service).await;

    let target_watermark: DateTime<Utc> = "2026-09-15T12:00:00Z".parse().unwrap();
    let (candidates, ref_map) = service
        .collect_recent_snapshot_candidates(target_watermark, 48)
        .await
        .expect("collect candidates");
    assert_eq!(candidates.len(), 2);

    let ref1 = &candidates[0].short_ref;
    let ref2 = &candidates[1].short_ref;

    // Gate 1: Coverage Incomplete (omitted s2)
    let incomplete_result = MemoryGenerationResultV2 {
        schema_version: 2,
        projects: vec![MemoryGenerationProjectV2 {
            project_key: candidates[0].project_key.clone(),
            summary: "Summary".to_string(),
            no_material_change: false,
            source_sessions: vec![ref1.clone()],
            items: vec![],
        }],
        coverage: MemoryGenerationCoverageV2 {
            covered_sessions: vec![ref1.clone()],
            no_memory_sessions: vec![],
            unreadable_sessions: vec![],
            budget_exhausted: false,
        },
        unknowns: vec![],
    };
    let err = service
        .execute_recent_memory_snapshot_pipeline(target_watermark, 48, incomplete_result)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not covered"));

    // Verify state has failure and last-success is None
    let state = service
        .get_recent_memory_snapshot()
        .await
        .expect("get state");
    assert_eq!(state.status, RecentMemoryStatus::UpdateFailed);
    assert!(state.snapshot.is_none());
    assert!(state.latest_attempt_error.is_some());

    // Gate 2: Budget exhausted
    let budget_exhausted_result = MemoryGenerationResultV2 {
        schema_version: 2,
        projects: vec![],
        coverage: MemoryGenerationCoverageV2 {
            covered_sessions: vec![ref1.clone(), ref2.clone()],
            no_memory_sessions: vec![],
            unreadable_sessions: vec![],
            budget_exhausted: true,
        },
        unknowns: vec![],
    };
    let err = service
        .validate_memory_generation_result(&budget_exhausted_result, &candidates, &ref_map)
        .unwrap_err();
    assert!(err.to_string().contains("budget exhausted"));

    // Gate 3: Invalid reference key
    let invalid_ref_result = MemoryGenerationResultV2 {
        schema_version: 2,
        projects: vec![MemoryGenerationProjectV2 {
            project_key: candidates[0].project_key.clone(),
            summary: "Summary".to_string(),
            no_material_change: false,
            source_sessions: vec![ref1.clone()],
            items: vec![MemoryGenerationItemV2 {
                continues_item_id: None,
                category: MemoryItemCategory::Progress,
                status: MemoryItemStatus::Active,
                title: "Item 1".to_string(),
                summary: "Item summary".to_string(),
                rationale: "Rationale".to_string(),
                occurred_at: "2026-09-15T01:00:00Z".to_string(),
                recommendation_rank: None,
                source_refs: vec!["unknown-ref-999".to_string()],
                promotion_nomination: MemoryPromotionNomination::None,
            }],
        }],
        coverage: MemoryGenerationCoverageV2 {
            covered_sessions: vec![ref1.clone(), ref2.clone()],
            no_memory_sessions: vec![],
            unreadable_sessions: vec![],
            budget_exhausted: false,
        },
        unknowns: vec![],
    };
    let err = service
        .validate_memory_generation_result(&invalid_ref_result, &candidates, &ref_map)
        .unwrap_err();
    assert!(err.to_string().contains("Unknown source reference"));

    // Gate 4: Duplicate recommendation rank
    let duplicate_rank_result = MemoryGenerationResultV2 {
        schema_version: 2,
        projects: vec![MemoryGenerationProjectV2 {
            project_key: candidates[0].project_key.clone(),
            summary: "Summary".to_string(),
            no_material_change: false,
            source_sessions: vec![ref1.clone()],
            items: vec![
                MemoryGenerationItemV2 {
                    continues_item_id: None,
                    category: MemoryItemCategory::Decision,
                    status: MemoryItemStatus::Verified,
                    title: "Rec 1".to_string(),
                    summary: "Summary".to_string(),
                    rationale: "Rationale".to_string(),
                    occurred_at: "2026-09-15T01:00:00Z".to_string(),
                    recommendation_rank: Some(1),
                    source_refs: vec![ref1.clone()],
                    promotion_nomination: MemoryPromotionNomination::None,
                },
                MemoryGenerationItemV2 {
                    continues_item_id: None,
                    category: MemoryItemCategory::FollowUp,
                    status: MemoryItemStatus::Active,
                    title: "Rec 2".to_string(),
                    summary: "Summary".to_string(),
                    rationale: "Rationale".to_string(),
                    occurred_at: "2026-09-15T01:00:00Z".to_string(),
                    recommendation_rank: Some(1), // duplicate!
                    source_refs: vec![ref1.clone()],
                    promotion_nomination: MemoryPromotionNomination::None,
                },
            ],
        }],
        coverage: MemoryGenerationCoverageV2 {
            covered_sessions: vec![ref1.clone(), ref2.clone()],
            no_memory_sessions: vec![],
            unreadable_sessions: vec![],
            budget_exhausted: false,
        },
        unknowns: vec![],
    };
    let err = service
        .validate_memory_generation_result(&duplicate_rank_result, &candidates, &ref_map)
        .unwrap_err();
    assert!(err.to_string().contains("Duplicate recommendation rank"));

    let expected_alpha_key = resolve_project_directory("/tmp/alpha-project", &[])
        .unwrap_or_else(|| "/tmp/alpha-project".to_string());
    let unassigned_nomination_result = MemoryGenerationResultV2 {
        schema_version: 2,
        projects: vec![
            MemoryGenerationProjectV2 {
                project_key: expected_alpha_key.clone(),
                summary: "Alpha summary".to_string(),
                no_material_change: false,
                source_sessions: vec![ref1.clone()],
                items: vec![],
            },
            MemoryGenerationProjectV2 {
                project_key: "unassigned".to_string(),
                summary: "Unassigned summary".to_string(),
                no_material_change: false,
                source_sessions: vec![ref2.clone()],
                items: vec![MemoryGenerationItemV2 {
                    continues_item_id: None,
                    category: MemoryItemCategory::Decision,
                    status: MemoryItemStatus::Verified,
                    title: "Unassigned decision".to_string(),
                    summary: "Summary".to_string(),
                    rationale: "Rationale".to_string(),
                    occurred_at: "2026-09-15T01:00:00Z".to_string(),
                    recommendation_rank: None,
                    source_refs: vec![ref2.clone()],
                    promotion_nomination: MemoryPromotionNomination::ProjectDecision, // Not allowed!
                }],
            },
        ],
        coverage: MemoryGenerationCoverageV2 {
            covered_sessions: vec![ref1.clone(), ref2.clone()],
            no_memory_sessions: vec![],
            unreadable_sessions: vec![],
            budget_exhausted: false,
        },
        unknowns: vec![],
    };
    let err = service
        .validate_memory_generation_result(&unassigned_nomination_result, &candidates, &ref_map)
        .unwrap_err();
    assert!(err
        .to_string()
        .contains("unassigned project cannot be nominated"));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_recent_snapshot_pipeline_success_atomicity_and_preservation() {
    let (service, _root) = setup_test_service().await;
    seed_test_conversation_data(&service).await;

    let target_watermark: DateTime<Utc> = "2026-09-15T12:00:00Z".parse().unwrap();
    let (candidates, _ref_map) = service
        .collect_recent_snapshot_candidates(target_watermark, 48)
        .await
        .expect("collect candidates");
    assert_eq!(candidates.len(), 2);

    let expected_alpha_key = resolve_project_directory("/tmp/alpha-project", &[])
        .unwrap_or_else(|| "/tmp/alpha-project".to_string());
    let alpha_candidate = candidates
        .iter()
        .find(|c| c.project_key == expected_alpha_key)
        .unwrap();
    let unassigned_candidate = candidates
        .iter()
        .find(|c| c.project_key == "unassigned")
        .unwrap();

    let valid_result = MemoryGenerationResultV2 {
        schema_version: 2,
        projects: vec![
            MemoryGenerationProjectV2 {
                project_key: expected_alpha_key.clone(),
                summary: "Alpha project updates and next steps.".to_string(),
                no_material_change: false,
                source_sessions: vec![alpha_candidate.short_ref.clone()],
                items: vec![
                    MemoryGenerationItemV2 {
                        continues_item_id: None,
                        category: MemoryItemCategory::Progress,
                        status: MemoryItemStatus::Active,
                        title: "Implemented database migrations".to_string(),
                        summary: "Completed schema foundation tables".to_string(),
                        rationale: "Needed for persistence".to_string(),
                        occurred_at: "2026-09-15T02:00:00Z".to_string(),
                        recommendation_rank: None,
                        source_refs: vec![alpha_candidate.short_ref.clone()],
                        promotion_nomination: MemoryPromotionNomination::None,
                    },
                    MemoryGenerationItemV2 {
                        continues_item_id: None,
                        category: MemoryItemCategory::Decision,
                        status: MemoryItemStatus::Verified,
                        title: "Decided to adopt single-tier symlinks".to_string(),
                        summary: "Direct symlinks from app dir to sources".to_string(),
                        rationale: "Reduces intermediate link complexity".to_string(),
                        occurred_at: "2026-09-15T02:00:00Z".to_string(),
                        recommendation_rank: Some(1),
                        source_refs: vec![alpha_candidate.short_ref.clone()],
                        promotion_nomination: MemoryPromotionNomination::ProjectDecision,
                    },
                ],
            },
            MemoryGenerationProjectV2 {
                project_key: "unassigned".to_string(),
                summary: "Unassigned exploratory sessions.".to_string(),
                no_material_change: false,
                source_sessions: vec![unassigned_candidate.short_ref.clone()],
                items: vec![MemoryGenerationItemV2 {
                    continues_item_id: None,
                    category: MemoryItemCategory::Blocker,
                    status: MemoryItemStatus::Blocked,
                    title: "Waiting on external API key".to_string(),
                    summary: "Cannot run tests without API key".to_string(),
                    rationale: "Third party vendor delay".to_string(),
                    occurred_at: "2026-09-14T06:00:00Z".to_string(),
                    recommendation_rank: Some(1),
                    source_refs: vec![unassigned_candidate.short_ref.clone()],
                    promotion_nomination: MemoryPromotionNomination::None,
                }],
            },
        ],
        coverage: MemoryGenerationCoverageV2 {
            covered_sessions: vec![
                alpha_candidate.short_ref.clone(),
                unassigned_candidate.short_ref.clone(),
            ],
            no_memory_sessions: vec![],
            unreadable_sessions: vec![],
            budget_exhausted: false,
        },
        unknowns: vec![],
    };

    // 1. Commit snapshot successfully
    let snapshot_view = service
        .execute_recent_memory_snapshot_pipeline(target_watermark, 48, valid_result.clone())
        .await
        .expect("execute pipeline successfully");

    assert_eq!(snapshot_view.window_hours, 48);
    assert_eq!(snapshot_view.projects.len(), 2);

    let alpha_proj = snapshot_view
        .projects
        .iter()
        .find(|p| p.project_key == expected_alpha_key)
        .unwrap();
    assert_eq!(alpha_proj.items.len(), 2);
    let rec_item = alpha_proj
        .items
        .iter()
        .find(|i| i.recommendation_rank == Some(1))
        .unwrap();
    assert_eq!(rec_item.title, "Decided to adopt single-tier symlinks");
    assert_eq!(rec_item.session_references.len(), 1);
    assert_eq!(
        rec_item.session_references[0].session_id,
        "session-in-alpha"
    );

    let unassigned_proj = snapshot_view
        .projects
        .iter()
        .find(|p| p.project_key == "unassigned")
        .unwrap();
    assert_eq!(unassigned_proj.items.len(), 1);

    // Verify promotion observations in DB
    let obs_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM memory_promotion_observations WHERE tenant_id = 'default'",
    )
    .fetch_one(service.db.pool())
    .await
    .expect("count observations");
    // Only alpha project item had promotion nomination (unassigned had None)
    assert_eq!(obs_count.0, 1);

    // 2. Query state via public get_recent_memory_snapshot API
    let state = service
        .get_recent_memory_snapshot()
        .await
        .expect("get state");
    assert_eq!(state.status, RecentMemoryStatus::Ready);
    assert!(state.latest_attempt_error.is_none());
    let current_snap = state.snapshot.expect("has snapshot");
    assert_eq!(current_snap.snapshot_id, snapshot_view.snapshot_id);

    // 3. Trigger a failure on subsequent pipeline run -> last success must be preserved!
    let failing_result = MemoryGenerationResultV2 {
        schema_version: 2,
        projects: vec![],
        coverage: MemoryGenerationCoverageV2 {
            covered_sessions: vec![],
            no_memory_sessions: vec![],
            unreadable_sessions: vec!["unreadable".to_string()],
            budget_exhausted: false,
        },
        unknowns: vec![],
    };

    let err = service
        .execute_recent_memory_snapshot_pipeline(target_watermark, 48, failing_result)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("Unreadable sessions"));

    let state_after_fail = service
        .get_recent_memory_snapshot()
        .await
        .expect("get state after fail");
    assert_eq!(state_after_fail.status, RecentMemoryStatus::UpdateFailed);
    assert!(state_after_fail.latest_attempt_error.is_some());
    let preserved_snap = state_after_fail.snapshot.expect("preserved snapshot");
    assert_eq!(preserved_snap.snapshot_id, snapshot_view.snapshot_id);

    // 4. Re-run pipeline with success on the same target watermark -> must overwrite without UNIQUE constraint error!
    let overwrite_snap = service
        .execute_recent_memory_snapshot_pipeline(target_watermark, 48, valid_result)
        .await
        .expect("overwrite snapshot on same watermark succeeds");
    assert_ne!(overwrite_snap.snapshot_id, snapshot_view.snapshot_id);

    let state_after_overwrite = service
        .get_recent_memory_snapshot()
        .await
        .expect("get state after overwrite");
    assert_eq!(state_after_overwrite.status, RecentMemoryStatus::Ready);
    assert!(state_after_overwrite.latest_attempt_error.is_none());
    assert_eq!(
        state_after_overwrite.snapshot.unwrap().snapshot_id,
        overwrite_snap.snapshot_id
    );
}

#[test]
fn test_watermark_resolution_defaults_and_custom() {
    use chrono::FixedOffset;

    let tz = FixedOffset::east_opt(8 * 3600).unwrap(); // UTC+8

    // Test default 02:00 / 14:00 at various times of day:
    // 1. At 01:59 UTC+8 on 2026-09-15 -> should pick yesterday 14:00 UTC+8 (2026-09-14 14:00)
    let now_0159 = tz.with_ymd_and_hms(2026, 9, 15, 1, 59, 0).unwrap();
    let target = resolve_target_watermark(now_0159, 48, "02:00", "14:00").unwrap();
    assert_eq!(target.local_watermark_date, "2026-09-14");
    assert_eq!(target.local_watermark_time, "14:00");
    assert_eq!(target.timezone_offset_minutes, 480);
    assert_eq!(target.window_hours, 48);

    // 2. Exactly at 02:00 UTC+8 on 2026-09-15 -> picks today 02:00
    let now_0200 = tz.with_ymd_and_hms(2026, 9, 15, 2, 0, 0).unwrap();
    let target = resolve_target_watermark(now_0200, 48, "02:00", "14:00").unwrap();
    assert_eq!(target.local_watermark_date, "2026-09-15");
    assert_eq!(target.local_watermark_time, "02:00");

    // 3. At 13:59 UTC+8 on 2026-09-15 -> still picks today 02:00
    let now_1359 = tz.with_ymd_and_hms(2026, 9, 15, 13, 59, 0).unwrap();
    let target = resolve_target_watermark(now_1359, 48, "02:00", "14:00").unwrap();
    assert_eq!(target.local_watermark_date, "2026-09-15");
    assert_eq!(target.local_watermark_time, "02:00");

    // 4. At 14:00 UTC+8 on 2026-09-15 -> picks today 14:00
    let now_1400 = tz.with_ymd_and_hms(2026, 9, 15, 14, 0, 0).unwrap();
    let target = resolve_target_watermark(now_1400, 48, "02:00", "14:00").unwrap();
    assert_eq!(target.local_watermark_date, "2026-09-15");
    assert_eq!(target.local_watermark_time, "14:00");

    // 5. At 23:59 UTC+8 on 2026-09-15 -> picks today 14:00
    let now_2359 = tz.with_ymd_and_hms(2026, 9, 15, 23, 59, 0).unwrap();
    let target = resolve_target_watermark(now_2359, 48, "02:00", "14:00").unwrap();
    assert_eq!(target.local_watermark_date, "2026-09-15");
    assert_eq!(target.local_watermark_time, "14:00");

    // Test custom watermarks: 03:30 and 15:30 with window 24h
    let target = resolve_target_watermark(now_1400, 24, "03:30", "15:30").unwrap();
    assert_eq!(target.local_watermark_date, "2026-09-15");
    assert_eq!(target.local_watermark_time, "03:30");
    assert_eq!(target.window_hours, 24);

    // Validation errors
    assert!(resolve_target_watermark(now_1400, 36, "02:00", "14:00").is_err());
    assert!(resolve_target_watermark(now_1400, 48, "02:00", "02:00").is_err());
    assert!(resolve_target_watermark(now_1400, 48, "24:00", "14:00").is_err());
    assert!(resolve_target_watermark(now_1400, 48, "2:00", "14:00").is_err());
}

#[test]
fn test_watermark_missed_and_dst() {
    use chrono::FixedOffset;

    let tz = FixedOffset::east_opt(8 * 3600).unwrap();

    // M35-L1-05: Missed watermark (app offline for 5 days)
    // System comes online at 2026-09-20 16:00
    // Should only resolve the latest expired watermark (2026-09-20 14:00), ignoring previous missed ones
    let now_online = tz.with_ymd_and_hms(2026, 9, 20, 16, 0, 0).unwrap();
    let target = resolve_target_watermark(now_online, 48, "02:00", "14:00").unwrap();
    assert_eq!(target.local_watermark_date, "2026-09-20");
    assert_eq!(target.local_watermark_time, "14:00");

    // DST simulation: resolve_local_time_with_dst
    let naive = chrono::NaiveDate::from_ymd_opt(2026, 3, 29)
        .unwrap()
        .and_hms_opt(2, 0, 0)
        .unwrap();

    let resolved = resolve_local_time_with_dst(&tz, naive);
    assert!(resolved.is_some());
}

#[test]
fn test_fingerprint_separation() {
    let skill = MemorySkillBinding {
        asset_id: "skill-gen".to_string(),
        asset_revision: 1,
        content_hash: "hash-123".to_string(),
        entry_hash: "entry-123".to_string(),
    };

    let cand1 = CandidateSession {
        tenant_id: "default".to_string(),
        session_id: "s1".to_string(),
        source_id: "src1".to_string(),
        session_title: "Session 1".to_string(),
        source_agent: "agent".to_string(),
        project_path: None,
        project_key: "unassigned".to_string(),
        last_activity_at: "2026-09-15T01:00:00Z".to_string(),
        source_revision: 1,
        short_ref: "s1".to_string(),
    };

    let dt1: DateTime<Utc> = "2026-09-15T02:00:00Z".parse().unwrap();
    let dt2: DateTime<Utc> = "2026-09-15T14:00:00Z".parse().unwrap();

    // 1. Same candidate sessions, different target watermarks:
    let target_fp_1 =
        compute_target_fingerprint("default", &dt1, 48, &[cand1.clone()], &[], &skill);
    let target_fp_2 =
        compute_target_fingerprint("default", &dt2, 48, &[cand1.clone()], &[], &skill);
    // Target fingerprints MUST DIFFER because target_watermark_utc differed:
    assert_ne!(target_fp_1, target_fp_2);

    // Content fingerprints MUST BE IDENTICAL because target_watermark_utc is NOT in content fingerprint:
    let content_fp_1 =
        compute_content_fingerprint(48, &[cand1.clone()], &[], &[], &skill, &[], &[]);
    let content_fp_2 =
        compute_content_fingerprint(48, &[cand1.clone()], &[], &[], &skill, &[], &[]);
    assert_eq!(content_fp_1, content_fp_2);

    // 2. Modifying candidate activity time changes content fingerprint:
    let mut cand2 = cand1.clone();
    cand2.last_activity_at = "2026-09-15T01:30:00Z".to_string();
    let content_fp_changed = compute_content_fingerprint(48, &[cand2], &[], &[], &skill, &[], &[]);
    assert_ne!(content_fp_1, content_fp_changed);

    // 3. Modifying exclusion changes content fingerprint:
    let content_fp_excluded = compute_content_fingerprint(
        48,
        &[cand1.clone()],
        &[],
        &[],
        &skill,
        &["s-other".to_string()],
        &[],
    );
    assert_ne!(content_fp_1, content_fp_excluded);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_dual_watermark_reuse_pipeline() {
    use chrono::FixedOffset;

    let (service, _root) = setup_test_service().await;

    sqlx::query(
        r#"
            INSERT INTO conversation_sources (
                tenant_id, id, adapter_id, name, kind, location, config_json, enabled,
                last_synced_at, last_sync_status, created_at, updated_at
            ) VALUES (
                'default', 'source-alpha', 'adapter-claude', 'Alpha Source', 'local_folder',
                '/tmp/source', '{}', 1, '2026-09-14T00:00:00Z', 'idle',
                '2026-09-14T00:00:00Z', '2026-09-14T00:00:00Z'
            )
            "#,
    )
    .execute(service.db.pool())
    .await
    .expect("insert source");

    // Insert two stable sessions whose activity falls within both the 02:00 and 14:00 48h windows
    insert_test_session(
        service.db.pool(),
        "session-alpha",
        "source-alpha",
        "Alpha Active Session",
        Some("/tmp/alpha-project"),
        "2026-09-14T12:00:00Z",
    )
    .await;

    insert_test_session(
        service.db.pool(),
        "session-unassigned",
        "source-alpha",
        "Unassigned Session",
        None,
        "2026-09-14T15:00:00Z",
    )
    .await;

    let tz = FixedOffset::east_opt(8 * 3600).unwrap();

    // Step 1: At 02:05 UTC+8, first watermark 02:00 has passed -> Generates snapshot 1
    let now_0205 = tz.with_ymd_and_hms(2026, 9, 15, 2, 5, 0).unwrap();

    let (candidates, _) = service
        .collect_recent_snapshot_candidates("2026-09-14T18:00:00Z".parse().unwrap(), 48)
        .await
        .expect("collect candidates");

    let alpha_cand = candidates
        .iter()
        .find(|c| c.project_key != "unassigned")
        .unwrap();
    let unassigned_cand = candidates
        .iter()
        .find(|c| c.project_key == "unassigned")
        .unwrap();

    let initial_result = MemoryGenerationResultV2 {
        schema_version: 2,
        projects: vec![
            MemoryGenerationProjectV2 {
                project_key: alpha_cand.project_key.clone(),
                summary: "Alpha project summary.".to_string(),
                no_material_change: false,
                source_sessions: vec![alpha_cand.short_ref.clone()],
                items: vec![MemoryGenerationItemV2 {
                    continues_item_id: None,
                    category: MemoryItemCategory::Progress,
                    status: MemoryItemStatus::Active,
                    title: "Initial work".to_string(),
                    summary: "Summary of work".to_string(),
                    rationale: "Rationale".to_string(),
                    occurred_at: "2026-09-14T20:00:00Z".to_string(),
                    recommendation_rank: Some(1),
                    source_refs: vec![alpha_cand.short_ref.clone()],
                    promotion_nomination: MemoryPromotionNomination::ProjectDecision,
                }],
            },
            MemoryGenerationProjectV2 {
                project_key: "unassigned".to_string(),
                summary: "Unassigned summary.".to_string(),
                no_material_change: true,
                source_sessions: vec![unassigned_cand.short_ref.clone()],
                items: vec![],
            },
        ],
        coverage: MemoryGenerationCoverageV2 {
            covered_sessions: vec![
                alpha_cand.short_ref.clone(),
                unassigned_cand.short_ref.clone(),
            ],
            no_memory_sessions: vec![],
            unreadable_sessions: vec![],
            budget_exhausted: false,
        },
        unknowns: vec![],
    };

    let snap1 = service
        .evaluate_and_run_recent_snapshot(Some(now_0205), Some(initial_result))
        .await
        .expect("evaluate snap 1")
        .expect("must produce snapshot 1");

    assert_eq!(
        snap1.publication_kind,
        RecentSnapshotPublicationKind::Generated
    );
    assert_eq!(snap1.reused_from_snapshot_id, None);
    let content_gen_at_1 = snap1.content_generated_at.clone();

    // Observation count in DB should be 1
    let obs_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM memory_promotion_observations WHERE tenant_id = 'default'",
    )
    .fetch_one(service.db.pool())
    .await
    .expect("count obs");
    assert_eq!(obs_count.0, 1);

    // Step 2: Running again at 02:10 UTC+8 (same watermark 02:00) -> Target idempotency, returns None
    let now_0210 = tz.with_ymd_and_hms(2026, 9, 15, 2, 10, 0).unwrap();
    let res_repeat = service
        .evaluate_and_run_recent_snapshot::<FixedOffset>(Some(now_0210), None)
        .await
        .expect("repeat target check");
    assert!(res_repeat.is_none());

    // Step 3: At 14:05 UTC+8, watermark advances to 14:00!
    // No session was added, so content_fingerprint is IDENTICAL!
    // Agent must NOT be called (mock_result is None)
    let now_1405 = tz.with_ymd_and_hms(2026, 9, 15, 14, 5, 0).unwrap();
    let snap2 = service
        .evaluate_and_run_recent_snapshot::<FixedOffset>(Some(now_1405), None)
        .await
        .expect("evaluate snap 2")
        .expect("must produce reused snapshot 2");

    // Verification of M35-L1-06 (Reuse):
    assert_eq!(
        snap2.publication_kind,
        RecentSnapshotPublicationKind::Reused
    );
    assert_eq!(
        snap2.reused_from_snapshot_id,
        Some(snap1.snapshot_id.clone())
    );
    // M35-L1-06: Preserves original content_generated_at!
    assert_eq!(snap2.content_generated_at, content_gen_at_1);
    // Sequence incremented
    assert_eq!(snap2.sequence, snap1.sequence + 1);
    // Projects and items copied
    assert_eq!(snap2.projects.len(), snap1.projects.len());
    let alpha_snap2 = snap2
        .projects
        .iter()
        .find(|p| p.project_key == alpha_cand.project_key)
        .unwrap();
    assert_eq!(alpha_snap2.items.len(), 1);
    assert_eq!(alpha_snap2.items[0].title, "Initial work");

    // M35-L1-06 / M35-L2-04: reused snapshot does NOT increment promotion observation count!
    let obs_count_after: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM memory_promotion_observations WHERE tenant_id = 'default'",
    )
    .fetch_one(service.db.pool())
    .await
    .expect("count obs after reuse");
    assert_eq!(obs_count_after.0, 1);

    // State check: Ready, pointing to snap2
    let state = service
        .get_recent_memory_snapshot()
        .await
        .expect("get state");
    assert_eq!(state.status, RecentMemoryStatus::Ready);
    assert_eq!(state.snapshot.unwrap().snapshot_id, snap2.snapshot_id);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_l1_item_continuation_and_7_day_limit() {
    use chrono::FixedOffset;

    let (service, _root) = setup_test_service().await;
    let pool = service.db.pool();

    // 1. Setup source and session
    sqlx::query(
        r#"
            INSERT INTO conversation_sources (
                tenant_id, id, adapter_id, name, kind, location, config_json, enabled,
                last_synced_at, last_sync_status, created_at, updated_at
            ) VALUES (
                'default', 'source-alpha', 'adapter-claude', 'Alpha Source', 'local_folder',
                '/tmp/source', '{}', 1, '2026-09-10T00:00:00Z', 'idle',
                '2026-09-10T00:00:00Z', '2026-09-10T00:00:00Z'
            )
            "#,
    )
    .execute(pool)
    .await
    .expect("insert source");

    insert_test_session(
        pool,
        "session-alpha",
        "source-alpha",
        "Alpha Session",
        Some("/tmp/alpha-project"),
        "2026-09-10T01:00:00Z",
    )
    .await;

    let tz = FixedOffset::east_opt(0).unwrap();

    // Step 1: Day 0 (2026-09-10T02:05:00Z) -> Generates Snapshot 1 with Active Item
    let now_day0 = tz.with_ymd_and_hms(2026, 9, 10, 2, 5, 0).unwrap();
    let (candidates, _) = service
        .collect_recent_snapshot_candidates("2026-09-10T02:00:00Z".parse().unwrap(), 48)
        .await
        .expect("collect candidates");

    let alpha_cand = candidates
        .iter()
        .find(|c| c.project_key != "unassigned")
        .unwrap();

    let initial_result = MemoryGenerationResultV2 {
        schema_version: 2,
        projects: vec![MemoryGenerationProjectV2 {
            project_key: alpha_cand.project_key.clone(),
            summary: "Initial day 0 work.".to_string(),
            no_material_change: false,
            source_sessions: vec![alpha_cand.short_ref.clone()],
            items: vec![MemoryGenerationItemV2 {
                continues_item_id: None,
                category: MemoryItemCategory::Progress,
                status: MemoryItemStatus::Active,
                title: "Feature A initial work".to_string(),
                summary: "Started feature A".to_string(),
                rationale: "Needed for milestone".to_string(),
                occurred_at: "2026-09-10T01:30:00Z".to_string(),
                recommendation_rank: None,
                source_refs: vec![alpha_cand.short_ref.clone()],
                promotion_nomination: MemoryPromotionNomination::None,
            }],
        }],
        coverage: MemoryGenerationCoverageV2 {
            covered_sessions: vec![alpha_cand.short_ref.clone()],
            no_memory_sessions: vec![],
            unreadable_sessions: vec![],
            budget_exhausted: false,
        },
        unknowns: vec![],
    };

    let snap1 = service
        .evaluate_and_run_recent_snapshot(Some(now_day0), Some(initial_result))
        .await
        .expect("snap 1")
        .expect("snap 1 some");

    let item1_id = snap1.projects[0].items[0].item_id.clone();

    // Verify Item 1 in DB
    let row1: (String, String, String, i64) = sqlx::query_as(
            "SELECT mi.first_seen_at, mi.last_seen_at, mi.lifecycle, mir.revision_number \
             FROM memory_items mi JOIN memory_item_revisions mir ON mi.current_revision_id = mir.id \
             WHERE mi.tenant_id = 'default' AND mi.id = ?1",
        )
        .bind(&item1_id)
        .fetch_one(pool)
        .await
        .expect("query item 1");

    assert_eq!(row1.2, "current");
    assert_eq!(row1.3, 1);
    let first_seen_at_day0 = row1.0.clone();

    // Step 2: Day 2 (2026-09-12T02:05:00Z)
    // Check collect_continuable_items at Day 2: item 1 must be present with remaining_days = 5
    let now_day2 = tz.with_ymd_and_hms(2026, 9, 12, 2, 5, 0).unwrap();
    let continuable_day2 = service
        .collect_continuable_items(&"2026-09-12T02:00:00Z".parse().unwrap())
        .await
        .expect("collect continuable day 2");

    let cont1 = continuable_day2
        .iter()
        .find(|i| i.item_id == item1_id)
        .expect("must find item1");
    assert_eq!(cont1.days_since_first_seen, 2);
    assert_eq!(cont1.remaining_days, 5);

    // Insert new activity at Day 2
    insert_test_session(
        pool,
        "session-alpha-day2",
        "source-alpha",
        "Alpha Session Day 2",
        Some("/tmp/alpha-project"),
        "2026-09-12T01:00:00Z",
    )
    .await;

    let (candidates_day2, _) = service
        .collect_recent_snapshot_candidates("2026-09-12T02:00:00Z".parse().unwrap(), 48)
        .await
        .expect("collect candidates day 2");
    let alpha_cand_day2 = candidates_day2
        .iter()
        .find(|c| c.project_key != "unassigned")
        .unwrap();

    // Agent continues item 1, updates status to Blocked
    let day2_result = MemoryGenerationResultV2 {
        schema_version: 2,
        projects: vec![MemoryGenerationProjectV2 {
            project_key: alpha_cand_day2.project_key.clone(),
            summary: "Day 2 work blocked.".to_string(),
            no_material_change: false,
            source_sessions: vec![alpha_cand_day2.short_ref.clone()],
            items: vec![MemoryGenerationItemV2 {
                continues_item_id: Some(item1_id.clone()),
                category: MemoryItemCategory::Blocker,
                status: MemoryItemStatus::Blocked,
                title: "Feature A blocked by dependency".to_string(),
                summary: "Feature A is now blocked".to_string(),
                rationale: "Upstream API change".to_string(),
                occurred_at: "2026-09-12T01:30:00Z".to_string(),
                recommendation_rank: None,
                source_refs: vec![alpha_cand_day2.short_ref.clone()],
                promotion_nomination: MemoryPromotionNomination::None,
            }],
        }],
        coverage: MemoryGenerationCoverageV2 {
            covered_sessions: vec![alpha_cand_day2.short_ref.clone()],
            no_memory_sessions: vec![],
            unreadable_sessions: vec![],
            budget_exhausted: false,
        },
        unknowns: vec![],
    };

    let snap2 = service
        .evaluate_and_run_recent_snapshot(Some(now_day2), Some(day2_result))
        .await
        .expect("snap 2")
        .expect("snap 2 some");

    assert_eq!(snap2.projects[0].items[0].item_id, item1_id);

    // Verify revision 2 and first_seen_at preserved
    let row2: (String, String, String, i64, Option<String>) = sqlx::query_as(
            "SELECT mi.first_seen_at, mi.last_seen_at, mi.lifecycle, mir.revision_number, mir.supersedes_revision_id \
             FROM memory_items mi JOIN memory_item_revisions mir ON mi.current_revision_id = mir.id \
             WHERE mi.tenant_id = 'default' AND mi.id = ?1",
        )
        .bind(&item1_id)
        .fetch_one(pool)
        .await
        .expect("query item 1 after day 2");

    // M35-L1-08: first_seen_at remains Day 0!
    assert_eq!(row2.0, first_seen_at_day0);
    // last_seen_at is updated to Day 2!
    assert_eq!(row2.1, "2026-09-12T02:00:00+00:00");
    assert_eq!(row2.2, "current");
    assert_eq!(row2.3, 2); // revision 2
    assert!(row2.4.is_some()); // supersedes rev 1

    // Step 3: Day 8 (> 7 days after first_seen_at)
    // 2026-09-18T02:00:00Z is 8 days after 2026-09-10T02:00:00Z
    let continuable_day8 = service
        .collect_continuable_items(&"2026-09-18T02:00:00Z".parse().unwrap())
        .await
        .expect("collect continuable day 8");

    // M35-L1-08: item 1 must be retired and NOT in continuable items!
    assert!(continuable_day8.iter().all(|i| i.item_id != item1_id));

    let lifecycle_day8: (String,) = sqlx::query_as(
        "SELECT lifecycle FROM memory_items WHERE tenant_id = 'default' AND id = ?1",
    )
    .bind(&item1_id)
    .fetch_one(pool)
    .await
    .expect("query item 1 lifecycle day 8");
    assert_eq!(lifecycle_day8.0, "retired");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_l1_terminal_state_displays_once_and_exits() {
    use chrono::FixedOffset;

    let (service, _root) = setup_test_service().await;
    let pool = service.db.pool();

    sqlx::query(
        r#"
            INSERT INTO conversation_sources (
                tenant_id, id, adapter_id, name, kind, location, config_json, enabled,
                last_synced_at, last_sync_status, created_at, updated_at
            ) VALUES (
                'default', 'source-beta', 'adapter-claude', 'Beta Source', 'local_folder',
                '/tmp/source', '{}', 1, '2026-09-10T00:00:00Z', 'idle',
                '2026-09-10T00:00:00Z', '2026-09-10T00:00:00Z'
            )
            "#,
    )
    .execute(pool)
    .await
    .expect("insert source");

    insert_test_session(
        pool,
        "session-beta-1",
        "source-beta",
        "Beta Session 1",
        Some("/tmp/beta-project"),
        "2026-09-10T01:00:00Z",
    )
    .await;

    let tz = FixedOffset::east_opt(0).unwrap();

    // Step 1: Snapshot 1: Item created as Active
    let now_1 = tz.with_ymd_and_hms(2026, 9, 10, 2, 5, 0).unwrap();
    let (cand1, _) = service
        .collect_recent_snapshot_candidates("2026-09-10T02:00:00Z".parse().unwrap(), 48)
        .await
        .expect("cand1");
    let beta_cand1 = cand1
        .iter()
        .find(|c| c.project_key != "unassigned")
        .unwrap();

    let res1 = MemoryGenerationResultV2 {
        schema_version: 2,
        projects: vec![MemoryGenerationProjectV2 {
            project_key: beta_cand1.project_key.clone(),
            summary: "Beta active task.".to_string(),
            no_material_change: false,
            source_sessions: vec![beta_cand1.short_ref.clone()],
            items: vec![MemoryGenerationItemV2 {
                continues_item_id: None,
                category: MemoryItemCategory::Progress,
                status: MemoryItemStatus::Active,
                title: "Feature B development".to_string(),
                summary: "Under development".to_string(),
                rationale: "Milestone B".to_string(),
                occurred_at: "2026-09-10T01:30:00Z".to_string(),
                recommendation_rank: None,
                source_refs: vec![beta_cand1.short_ref.clone()],
                promotion_nomination: MemoryPromotionNomination::None,
            }],
        }],
        coverage: MemoryGenerationCoverageV2 {
            covered_sessions: vec![beta_cand1.short_ref.clone()],
            no_memory_sessions: vec![],
            unreadable_sessions: vec![],
            budget_exhausted: false,
        },
        unknowns: vec![],
    };

    let snap1 = service
        .evaluate_and_run_recent_snapshot(Some(now_1), Some(res1))
        .await
        .expect("snap 1")
        .unwrap();
    let item_id = snap1.projects[0].items[0].item_id.clone();

    // Step 2: Snapshot 2: Item is Completed (terminal)
    insert_test_session(
        pool,
        "session-beta-2",
        "source-beta",
        "Beta Session 2",
        Some("/tmp/beta-project"),
        "2026-09-10T13:00:00Z",
    )
    .await;

    let now_2 = tz.with_ymd_and_hms(2026, 9, 10, 14, 5, 0).unwrap();
    let (cand2, _) = service
        .collect_recent_snapshot_candidates("2026-09-10T14:00:00Z".parse().unwrap(), 48)
        .await
        .expect("cand2");
    let beta_cand2 = cand2
        .iter()
        .find(|c| c.project_key != "unassigned")
        .unwrap();

    let res2 = MemoryGenerationResultV2 {
        schema_version: 2,
        projects: vec![MemoryGenerationProjectV2 {
            project_key: beta_cand2.project_key.clone(),
            summary: "Beta task completed.".to_string(),
            no_material_change: false,
            source_sessions: vec![beta_cand2.short_ref.clone()],
            items: vec![MemoryGenerationItemV2 {
                continues_item_id: Some(item_id.clone()),
                category: MemoryItemCategory::Progress,
                status: MemoryItemStatus::Completed,
                title: "Feature B development finished".to_string(),
                summary: "Completed and merged".to_string(),
                rationale: "Done".to_string(),
                occurred_at: "2026-09-10T13:30:00Z".to_string(),
                recommendation_rank: None,
                source_refs: vec![beta_cand2.short_ref.clone()],
                promotion_nomination: MemoryPromotionNomination::None,
            }],
        }],
        coverage: MemoryGenerationCoverageV2 {
            covered_sessions: cand2.iter().map(|c| c.short_ref.clone()).collect(),
            no_memory_sessions: vec![],
            unreadable_sessions: vec![],
            budget_exhausted: false,
        },
        unknowns: vec![],
    };

    let snap2 = service
        .evaluate_and_run_recent_snapshot(Some(now_2), Some(res2))
        .await
        .expect("snap 2")
        .unwrap();

    // M35-L1-09: In the snapshot where it becomes terminal, it displays once!
    assert_eq!(snap2.projects[0].items.len(), 1);
    assert_eq!(snap2.projects[0].items[0].item_id, item_id);
    assert_eq!(snap2.projects[0].items[0].status, "completed");

    // In DB, its lifecycle is now 'retired'
    let lifecycle: (String,) = sqlx::query_as(
        "SELECT lifecycle FROM memory_items WHERE tenant_id = 'default' AND id = ?1",
    )
    .bind(&item_id)
    .fetch_one(pool)
    .await
    .expect("lifecycle check");
    assert_eq!(lifecycle.0, "retired");

    // Step 3: Snapshot 3: Next day
    // collect_continuable_items must NOT return the completed item!
    let continuable_step3 = service
        .collect_continuable_items(&"2026-09-11T02:00:00Z".parse().unwrap())
        .await
        .expect("continuable step 3");
    assert!(continuable_step3.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_source_invalidation_retires_unpromoted_l1_item() {
    use chrono::FixedOffset;

    let (service, _root) = setup_test_service().await;
    let pool = service.db.pool();

    sqlx::query(
        r#"
            INSERT INTO conversation_sources (
                tenant_id, id, adapter_id, name, kind, location, config_json, enabled,
                last_synced_at, last_sync_status, created_at, updated_at
            ) VALUES (
                'default', 'source-gamma', 'adapter-claude', 'Gamma Source', 'local_folder',
                '/tmp/source', '{}', 1, '2026-09-10T00:00:00Z', 'idle',
                '2026-09-10T00:00:00Z', '2026-09-10T00:00:00Z'
            )
            "#,
    )
    .execute(pool)
    .await
    .expect("insert source");

    insert_test_session(
        pool,
        "session-gamma",
        "source-gamma",
        "Gamma Session",
        Some("/tmp/gamma-project"),
        "2026-09-10T01:00:00Z",
    )
    .await;

    let tz = FixedOffset::east_opt(0).unwrap();
    let now = tz.with_ymd_and_hms(2026, 9, 10, 2, 5, 0).unwrap();

    let (candidates, _) = service
        .collect_recent_snapshot_candidates("2026-09-10T02:00:00Z".parse().unwrap(), 48)
        .await
        .expect("candidates");
    let gamma_cand = candidates
        .iter()
        .find(|c| c.project_key != "unassigned")
        .unwrap();

    let res = MemoryGenerationResultV2 {
        schema_version: 2,
        projects: vec![MemoryGenerationProjectV2 {
            project_key: gamma_cand.project_key.clone(),
            summary: "Gamma task.".to_string(),
            no_material_change: false,
            source_sessions: vec![gamma_cand.short_ref.clone()],
            items: vec![MemoryGenerationItemV2 {
                continues_item_id: None,
                category: MemoryItemCategory::Progress,
                status: MemoryItemStatus::Active,
                title: "Gamma Item".to_string(),
                summary: "Gamma summary".to_string(),
                rationale: "Gamma rationale".to_string(),
                occurred_at: "2026-09-10T01:30:00Z".to_string(),
                recommendation_rank: None,
                source_refs: vec![gamma_cand.short_ref.clone()],
                promotion_nomination: MemoryPromotionNomination::None,
            }],
        }],
        coverage: MemoryGenerationCoverageV2 {
            covered_sessions: vec![gamma_cand.short_ref.clone()],
            no_memory_sessions: vec![],
            unreadable_sessions: vec![],
            budget_exhausted: false,
        },
        unknowns: vec![],
    };

    let snap = service
        .evaluate_and_run_recent_snapshot(Some(now), Some(res))
        .await
        .expect("eval")
        .unwrap();
    let l1_item_id = snap.projects[0].items[0].item_id.clone();

    // Also insert an L2 item to verify M35-L3-04 (L2 item is NOT retired on source invalidation)
    let l2_item_id = "item-l2-test".to_string();
    let l2_rev_id = "rev-l2-test".to_string();
    sqlx::query(
            "INSERT INTO memory_items (\
                tenant_id, id, layer, project_key, current_revision_id, lifecycle, \
                first_seen_at, last_seen_at, created_at, updated_at\
             ) VALUES ('default', ?1, 'l2', 'gamma', ?2, 'current', '2026-09-10T00:00:00Z', '2026-09-10T00:00:00Z', '2026-09-10T00:00:00Z', '2026-09-10T00:00:00Z')",
        )
        .bind(&l2_item_id)
        .bind(&l2_rev_id)
        .execute(pool)
        .await
        .expect("insert l2 item");

    sqlx::query(
            "INSERT INTO memory_item_revisions (\
                tenant_id, id, item_id, revision_number, category, status, title, summary, \
                rationale, recommendation_rank, promotion_nomination, occurred_at, \
                evidence_fingerprint, generated_by_snapshot_id, created_at\
             ) VALUES ('default', ?1, ?2, 1, 'decision', 'active', 'L2 Rule', 'L2 summary', 'L2 rat', NULL, 'none', '2026-09-10T00:00:00Z', 'fp', NULL, '2026-09-10T00:00:00Z')",
        )
        .bind(&l2_rev_id)
        .bind(&l2_item_id)
        .execute(pool)
        .await
        .expect("insert l2 rev");

    sqlx::query(
            "INSERT INTO memory_item_source_references (\
                tenant_id, id, item_revision_id, record_kind, source_id, session_id, \
                reference_key, source_revision, availability, created_at\
             ) VALUES ('default', 'ref-l2', ?1, 'session', 'source-gamma', 'session-gamma', 'source-gamma/session-gamma', 1, 'available', '2026-09-10T00:00:00Z')",
        )
        .bind(&l2_rev_id)
        .execute(pool)
        .await
        .expect("insert l2 ref");

    // Now disable source-gamma:
    sqlx::query("UPDATE conversation_sources SET enabled = 0 WHERE id = 'source-gamma'")
        .execute(pool)
        .await
        .expect("disable source");

    // Run sync
    service
        .sync_source_availability_and_retire_unpromoted_items("2026-09-11T00:00:00Z")
        .await
        .expect("sync source availability");

    // 1. Unpromoted L1 item's references are unavailable
    let l1_ref_status: (String, Option<String>) = sqlx::query_as(
            "SELECT availability, unavailable_reason FROM memory_item_source_references WHERE item_revision_id = (SELECT current_revision_id FROM memory_items WHERE id = ?1)",
        )
        .bind(&l1_item_id)
        .fetch_one(pool)
        .await
        .expect("l1 ref status");
    assert_eq!(l1_ref_status.0, "unavailable");
    assert_eq!(l1_ref_status.1.as_deref(), Some("source_disabled"));

    // 2. Unpromoted L1 item is retired
    let l1_lifecycle: (String,) =
        sqlx::query_as("SELECT lifecycle FROM memory_items WHERE id = ?1")
            .bind(&l1_item_id)
            .fetch_one(pool)
            .await
            .expect("l1 lifecycle");
    assert_eq!(l1_lifecycle.0, "retired");

    // 3. M35-L3-04: L2 item reference is unavailable, but L2 item lifecycle is STILL 'current'
    let l2_ref_status: (String, Option<String>) = sqlx::query_as(
            "SELECT availability, unavailable_reason FROM memory_item_source_references WHERE item_revision_id = ?1",
        )
        .bind(&l2_rev_id)
        .fetch_one(pool)
        .await
        .expect("l2 ref status");
    assert_eq!(l2_ref_status.0, "unavailable");
    assert_eq!(l2_ref_status.1.as_deref(), Some("source_disabled"));

    let l2_lifecycle: (String,) =
        sqlx::query_as("SELECT lifecycle FROM memory_items WHERE id = ?1")
            .bind(&l2_item_id)
            .fetch_one(pool)
            .await
            .expect("l2 lifecycle");
    assert_eq!(l2_lifecycle.0, "current");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_work_order_evidence_pack_is_structured_facts_without_markdown() {
    let (service, _root) = setup_test_service().await;
    let target = WatermarkTarget {
        target_watermark_utc: "2026-09-15T02:00:00Z".parse().unwrap(),
        local_watermark_date: "2026-09-15".to_string(),
        local_watermark_time: "02:00".to_string(),
        timezone_offset_minutes: 0,
        window_hours: 48,
        window_start_utc: "2026-09-13T02:00:00Z".parse().unwrap(),
        window_end_utc: "2026-09-15T02:00:00Z".parse().unwrap(),
    };

    let candidates = vec![CandidateSession {
        tenant_id: "default".to_string(),
        session_id: "s1".to_string(),
        source_id: "src1".to_string(),
        session_title: "Session 1".to_string(),
        source_agent: "agent".to_string(),
        project_key: "proj1".to_string(),
        project_path: None,
        last_activity_at: "2026-09-14T10:00:00Z".to_string(),
        source_revision: 1,
        short_ref: "SES-1".to_string(),
    }];

    let continuable = vec![ContinuableMemoryItemView {
        item_id: "item1".to_string(),
        project_key: "proj1".to_string(),
        category: MemoryItemCategory::Blocker,
        status: MemoryItemStatus::Blocked,
        title: "Task Blocked".to_string(),
        summary: "Blocked summary".to_string(),
        rationale: "Rationale".to_string(),
        first_seen_at: "2026-09-13T02:00:00Z".to_string(),
        last_seen_at: "2026-09-14T02:00:00Z".to_string(),
        days_since_first_seen: 1,
        remaining_days: 6,
        current_revision_id: "rev1".to_string(),
        current_revision_number: 1,
        evidence_fingerprint: "fp1".to_string(),
        source_refs: vec!["SES-1".to_string()],
    }];

    let pack =
        service.build_recent_snapshot_work_order_evidence_pack(&target, &candidates, &continuable);

    assert_eq!(pack.project_keys, vec!["proj1".to_string()]);
    assert_eq!(pack.candidate_sessions.len(), 1);
    assert_eq!(pack.candidate_sessions[0].session_ref, "SES-1");
    assert_eq!(pack.candidate_sessions[0].source_agent, "agent");
    assert_eq!(pack.candidate_sessions[0].source_revision, 1);
    assert_eq!(pack.session_evidence.len(), 1);
    assert_eq!(pack.session_evidence[0].candidate.session_id, "s1");
    assert!(pack.session_evidence[0].summary.is_none());
    assert_eq!(pack.continuable_items.len(), 1);
    assert_eq!(pack.continuable_items[0].remaining_days, 6);
    assert_eq!(pack.allowed_tools.len(), 4);

    // M35-L1-10: Verify serialized JSON has zero markdown fields
    let json_str = serde_json::to_string(&pack).expect("serialize pack");
    assert!(!json_str.contains(".md"));
    assert!(!json_str.contains("memory_summary"));
}
