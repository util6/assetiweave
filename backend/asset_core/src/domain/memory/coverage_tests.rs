use super::*;
use std::collections::{BTreeMap, HashMap, HashSet};

fn dummy_candidate(id: &str) -> CandidateSession {
    CandidateSession {
        tenant_id: "default".to_string(),
        session_id: id.to_string(),
        source_id: "source-1".to_string(),
        session_title: format!("Title {}", id),
        source_agent: "agent".to_string(),
        project_key: "proj-1".to_string(),
        project_path: None,
        last_activity_at: "2026-10-14T00:00:00Z".to_string(),
        source_revision: 1,
        short_ref: "s1".to_string(),
    }
}

fn dummy_memory(session_id: &str) -> SessionMemory {
    SessionMemory {
        tenant_id: "default".to_string(),
        id: format!("mem-{}", session_id),
        session_id: session_id.to_string(),
        source_id: "source-1".to_string(),
        source_revision: 1,
        source_fingerprint: "fp".to_string(),
        contract_version: "v2".to_string(),
        prompt_version: "v2".to_string(),
        status: SessionMemoryStatus::Active,
        project_path: None,
        summary: "Summary".to_string(),
        goal: "Goal".to_string(),
        result: "Result".to_string(),
        decisions: vec![],
        verification: vec![],
        blockers: vec![],
        follow_up: vec![],
        topics: vec![],
        generated_at: "2026-10-14T00:00:00Z".to_string(),
        created_at: "2026-10-14T00:00:00Z".to_string(),
        updated_at: "2026-10-14T00:00:00Z".to_string(),
        recipe_id: None,
        recipe_content_hash: None,
        work_order_json: None,
    }
}

fn dummy_reference(session_id: &str) -> SessionMemorySourceReference {
    SessionMemorySourceReference {
        tenant_id: "default".to_string(),
        id: format!("ref-id-{}", session_id),
        memory_id: format!("mem-{}", session_id),
        source_id: "source-1".to_string(),
        session_id: session_id.to_string(),
        question_id: Some("q-1".to_string()),
        turn_id: None,
        part_id: None,
        node_id: None,
        node_order: Some(1),
        reference_key: format!("ref-{}", session_id),
        source_revision: 1,
        created_at: "2026-10-14T00:00:00Z".to_string(),
    }
}

#[test]
fn test_evaluate_ready_candidate() {
    let c = dummy_candidate("s-1");
    let mut mems = BTreeMap::new();
    mems.insert("s-1".to_string(), dummy_memory("s-1"));
    let mut refs = BTreeMap::new();
    refs.insert("s-1".to_string(), vec![dummy_reference("s-1")]);

    let proj =
        SessionCoverageProjection::evaluate(vec![c], &mems, &refs, &HashMap::new(), &[], &[]);

    assert_eq!(proj.ready_sessions.len(), 1);
    assert!(proj.transient_sessions.is_empty());
    assert!(proj.quarantined_sessions.is_empty());
    assert!(proj.is_transient_empty());
}

#[test]
fn test_evaluate_lease_expired_transient_candidate() {
    let c = dummy_candidate("s-lease");
    let mut jobs = HashMap::new();
    jobs.insert(
        "s-lease".to_string(),
        SessionJobEvaluationContext {
            status: "failed".to_string(),
            attempt_count: 2, // 未超限 ( < 10 )
            retry_count: 2,
            last_error: Some("lease_expired".to_string()),
        },
    );

    let proj = SessionCoverageProjection::evaluate(
        vec![c],
        &BTreeMap::new(),
        &BTreeMap::new(),
        &jobs,
        &[],
        &[],
    );

    assert!(proj.ready_sessions.is_empty());
    assert_eq!(proj.transient_sessions.len(), 1);
    assert_eq!(proj.transient_sessions[0].session_id, "s-lease");
    assert!(proj.quarantined_sessions.is_empty());
    assert!(!proj.is_transient_empty());
}

#[test]
fn test_evaluate_poison_and_excluded_quarantined_candidates() {
    let c_poison = dummy_candidate("s-poison");
    let c_excluded = dummy_candidate("s-excluded");

    let mut jobs = HashMap::new();
    jobs.insert(
        "s-poison".to_string(),
        SessionJobEvaluationContext {
            status: "failed".to_string(),
            attempt_count: 10, // 超限
            retry_count: 5,
            last_error: Some("lease_expired_limit_exceeded".to_string()),
        },
    );

    let excluded_sessions = vec!["s-excluded".to_string()];

    let proj = SessionCoverageProjection::evaluate(
        vec![c_poison, c_excluded],
        &BTreeMap::new(),
        &BTreeMap::new(),
        &jobs,
        &excluded_sessions,
        &[],
    );

    assert!(proj.ready_sessions.is_empty());
    assert!(proj.transient_sessions.is_empty());
    assert_eq!(proj.quarantined_sessions.len(), 2);
    assert!(proj.is_transient_empty());

    let summary = proj.quarantined_summary();
    assert_eq!(summary.len(), 2);
}
