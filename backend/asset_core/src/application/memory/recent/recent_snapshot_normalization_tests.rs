use std::collections::HashMap;

use super::*;
use crate::backend::domain::{
    CandidateSession, MemoryGenerationCoverage, MemoryGenerationItem, MemoryGenerationProject,
    MemoryGenerationResult, MemoryItemCategory, MemoryItemStatus, MemoryPromotionNomination,
    ResolvedEvidenceRef,
};

fn create_test_candidate(short_ref: &str, session_id: &str, project_key: &str) -> CandidateSession {
    CandidateSession {
        tenant_id: "default".to_string(),
        session_id: session_id.to_string(),
        source_id: "src1".to_string(),
        session_title: format!("Title {short_ref}"),
        source_agent: "agent".to_string(),
        project_key: project_key.to_string(),
        project_path: None,
        last_activity_at: "2026-09-20T10:00:00Z".to_string(),
        source_revision: 1,
        short_ref: short_ref.to_string(),
    }
}

#[test]
fn test_normalize_coverage_resolves_duplicate_no_memory_sessions() {
    let candidates = vec![create_test_candidate("s1", "session-1", "proj1")];
    let mut result = MemoryGenerationResult {
        schema_version: 2,
        projects: vec![MemoryGenerationProject {
            project_key: "proj1".to_string(),
            summary: "No change".to_string(),
            no_material_change: true,
            source_sessions: vec!["s1".to_string()],
            items: vec![],
        }],
        coverage: MemoryGenerationCoverage {
            covered_sessions: vec![],
            no_memory_sessions: vec!["s1".to_string(), "s1".to_string()],
            unreadable_sessions: vec![],
            budget_exhausted: false,
        },
        unknowns: vec![],
    };

    normalize_agent_memory_generation_result(&mut result, &candidates, &HashMap::new());

    assert_eq!(result.coverage.no_memory_sessions, vec!["s1".to_string()]);
    assert!(result.coverage.covered_sessions.is_empty());
}

#[test]
fn test_normalize_coverage_resolves_overlapping_covered_and_no_memory_sessions() {
    // Model returns s1 in BOTH covered_sessions AND no_memory_sessions (very common when model confuses 'covered' with 'all processed')
    let candidates = vec![create_test_candidate("s1", "session-1", "proj1")];
    let mut result = MemoryGenerationResult {
        schema_version: 2,
        projects: vec![MemoryGenerationProject {
            project_key: "proj1".to_string(),
            summary: "No change".to_string(),
            no_material_change: true,
            source_sessions: vec!["s1".to_string()],
            items: vec![],
        }],
        coverage: MemoryGenerationCoverage {
            covered_sessions: vec!["s1".to_string()],
            no_memory_sessions: vec!["s1".to_string()],
            unreadable_sessions: vec![],
            budget_exhausted: false,
        },
        unknowns: vec![],
    };

    normalize_agent_memory_generation_result(&mut result, &candidates, &HashMap::new());

    // Since s1 had no items, it belongs to no_memory_sessions and MUST NOT be in covered_sessions
    assert_eq!(result.coverage.no_memory_sessions, vec!["s1".to_string()]);
    assert!(result.coverage.covered_sessions.is_empty());
}

#[test]
fn test_normalize_coverage_keeps_item_session_in_covered_sessions() {
    // If a session actually produced items, even if model put it in no_memory_sessions, it MUST be in covered_sessions
    let candidates = vec![create_test_candidate("s1", "session-1", "proj1")];
    let mut ref_map = HashMap::new();
    ref_map.insert(
        "s1.r1".to_string(),
        ResolvedEvidenceRef {
            short_ref: "s1.r1".to_string(),
            source_id: "src1".to_string(),
            session_id: "session-1".to_string(),
            project_key: "proj1".to_string(),
            session_title: "Title s1".to_string(),
            source_agent: "agent".to_string(),
            last_activity_at: "2026-09-20T10:00:00Z".to_string(),
            reference_key: "ref-1".to_string(),
            source_revision: 1,
            question_id: None,
            turn_id: None,
            node_id: None,
        },
    );

    let mut result = MemoryGenerationResult {
        schema_version: 2,
        projects: vec![MemoryGenerationProject {
            project_key: "proj1".to_string(),
            summary: "Active project".to_string(),
            no_material_change: false,
            source_sessions: vec!["s1".to_string()],
            items: vec![MemoryGenerationItem {
                continues_item_id: None,
                category: MemoryItemCategory::Progress,
                status: MemoryItemStatus::Active,
                title: "Progress Item".to_string(),
                summary: "Summary".to_string(),
                rationale: "Rationale".to_string(),
                occurred_at: "2026-09-20T10:00:00Z".to_string(),
                recommendation_rank: None,
                source_refs: vec!["s1.r1".to_string()],
                promotion_nomination: MemoryPromotionNomination::None,
            }],
        }],
        coverage: MemoryGenerationCoverage {
            covered_sessions: vec!["s1".to_string()],
            no_memory_sessions: vec!["s1".to_string()],
            unreadable_sessions: vec![],
            budget_exhausted: false,
        },
        unknowns: vec![],
    };

    normalize_agent_memory_generation_result(&mut result, &candidates, &ref_map);

    assert_eq!(result.coverage.covered_sessions, vec!["s1".to_string()]);
    assert!(result.coverage.no_memory_sessions.is_empty());
}

#[test]
fn test_normalize_coverage_filters_hallucinated_and_trims_whitespace() {
    let candidates = vec![create_test_candidate("s1", "session-1", "proj1")];
    let mut result = MemoryGenerationResult {
        schema_version: 2,
        projects: vec![MemoryGenerationProject {
            project_key: "proj1".to_string(),
            summary: "No change".to_string(),
            no_material_change: true,
            source_sessions: vec!["s1".to_string()],
            items: vec![],
        }],
        coverage: MemoryGenerationCoverage {
            covered_sessions: vec!["  s1  ".to_string(), "s99_hallucinated".to_string()],
            no_memory_sessions: vec![],
            unreadable_sessions: vec![],
            budget_exhausted: false,
        },
        unknowns: vec![],
    };

    normalize_agent_memory_generation_result(&mut result, &candidates, &HashMap::new());

    assert_eq!(result.coverage.covered_sessions, vec!["s1".to_string()]);
    assert!(result.coverage.no_memory_sessions.is_empty());
}

#[test]
fn test_normalize_coverage_resolves_full_session_id_alias() {
    let candidates = vec![create_test_candidate(
        "s1",
        "conversation-session-long-uuid",
        "proj1",
    )];
    let mut result = MemoryGenerationResult {
        schema_version: 2,
        projects: vec![MemoryGenerationProject {
            project_key: "proj1".to_string(),
            summary: "No change".to_string(),
            no_material_change: true,
            source_sessions: vec!["s1".to_string()],
            items: vec![],
        }],
        coverage: MemoryGenerationCoverage {
            covered_sessions: vec![],
            no_memory_sessions: vec!["conversation-session-long-uuid".to_string()],
            unreadable_sessions: vec![],
            budget_exhausted: false,
        },
        unknowns: vec![],
    };

    normalize_agent_memory_generation_result(&mut result, &candidates, &HashMap::new());

    assert_eq!(result.coverage.no_memory_sessions, vec!["s1".to_string()]);
    assert!(result.coverage.covered_sessions.is_empty());
}

#[test]
fn test_normalize_coverage_partitions_unmentioned_candidates() {
    let candidates = vec![
        create_test_candidate("s1", "session-1", "proj1"),
        create_test_candidate("s2", "session-2", "proj2"),
    ];
    let mut result = MemoryGenerationResult {
        schema_version: 2,
        projects: vec![
            MemoryGenerationProject {
                project_key: "proj1".to_string(),
                summary: "Active".to_string(),
                no_material_change: false,
                source_sessions: vec!["s1".to_string()],
                items: vec![MemoryGenerationItem {
                    continues_item_id: None,
                    category: MemoryItemCategory::Progress,
                    status: MemoryItemStatus::Active,
                    title: "Item".to_string(),
                    summary: "Sum".to_string(),
                    rationale: "Rat".to_string(),
                    occurred_at: "2026-09-20T10:00:00Z".to_string(),
                    recommendation_rank: None,
                    source_refs: vec!["s1.r1".to_string()],
                    promotion_nomination: MemoryPromotionNomination::None,
                }],
            },
            MemoryGenerationProject {
                project_key: "proj2".to_string(),
                summary: "No change".to_string(),
                no_material_change: true,
                source_sessions: vec!["s2".to_string()],
                items: vec![],
            },
        ],
        coverage: MemoryGenerationCoverage::default(), // model completely omitted coverage
        unknowns: vec![],
    };

    let mut ref_map = HashMap::new();
    ref_map.insert(
        "s1.r1".to_string(),
        ResolvedEvidenceRef {
            short_ref: "s1.r1".to_string(),
            source_id: "src1".to_string(),
            session_id: "session-1".to_string(),
            project_key: "proj1".to_string(),
            session_title: "Title s1".to_string(),
            source_agent: "agent".to_string(),
            last_activity_at: "2026-09-20T10:00:00Z".to_string(),
            reference_key: "ref-1".to_string(),
            source_revision: 1,
            question_id: None,
            turn_id: None,
            node_id: None,
        },
    );

    normalize_agent_memory_generation_result(&mut result, &candidates, &ref_map);

    assert_eq!(result.coverage.covered_sessions, vec!["s1".to_string()]);
    assert_eq!(result.coverage.no_memory_sessions, vec!["s2".to_string()]);
}

#[test]
fn test_agent_result_normalization_drops_unassigned_promotion_nomination() {
    let mut result = MemoryGenerationResult {
        schema_version: 2,
        projects: vec![MemoryGenerationProject {
            project_key: "unassigned".to_string(),
            summary: "Summary".to_string(),
            no_material_change: false,
            source_sessions: vec!["s1".to_string()],
            items: vec![MemoryGenerationItem {
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
        coverage: MemoryGenerationCoverage::default(),
        unknowns: Vec::new(),
    };

    normalize_agent_memory_generation_result(&mut result, &[], &HashMap::new());

    assert_eq!(
        result.projects[0].items[0].promotion_nomination,
        MemoryPromotionNomination::None
    );
}
