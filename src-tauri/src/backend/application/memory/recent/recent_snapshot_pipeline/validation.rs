use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::memory::MemoryPromotionNomination;
use crate::backend::store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row, Sqlite};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

impl AppService {
    pub(crate) fn validate_memory_generation_result(
        &self,
        result: &MemoryGenerationResult,
        candidates: &[CandidateSession],
        ref_map: &HashMap<String, ResolvedEvidenceRef>,
    ) -> AppResult<()> {
        if result.schema_version != 2 {
            return Err(AppError::Validation(format!(
                "Unsupported memory generation schema version: {}",
                result.schema_version
            )));
        }

        // 1. Coverage 门禁: 预算耗尽或不可读 Session 发生时直接拒绝发布
        if result.coverage.budget_exhausted {
            return Err(AppError::Domain {
                code: "MEMORY_BUDGET_EXHAUSTED".to_string(),
                message: "Memory generation budget exhausted, refusing publication".to_string(),
                retryable: false,
                details: None,
            });
        }
        if !result.coverage.unreadable_sessions.is_empty() {
            return Err(AppError::Domain {
                code: "MEMORY_COVERAGE_INCOMPLETE".to_string(),
                message: format!(
                    "Unreadable sessions detected: {:?}",
                    result.coverage.unreadable_sessions
                ),
                retryable: true,
                details: None,
            });
        }

        // Coverage must be an exact partition of the frozen candidate set.
        // Do not allow an Agent to invent a short ref, silently duplicate a
        // session, or omit a candidate while still returning a valid shape.
        let candidate_by_ref = candidates
            .iter()
            .map(|candidate| (candidate.short_ref.as_str(), candidate))
            .collect::<HashMap<_, _>>();
        let mut covered_set = HashSet::new();
        for short_ref in &result.coverage.covered_sessions {
            if !candidate_by_ref.contains_key(short_ref.as_str()) || !covered_set.insert(short_ref)
            {
                return Err(AppError::Validation(format!(
                    "Invalid or duplicate covered session reference '{short_ref}'"
                )));
            }
        }
        for short_ref in &result.coverage.no_memory_sessions {
            if !candidate_by_ref.contains_key(short_ref.as_str()) || !covered_set.insert(short_ref)
            {
                return Err(AppError::Validation(format!(
                    "Invalid or duplicate no-memory session reference '{short_ref}'"
                )));
            }
        }

        for candidate in candidates {
            if !covered_set.contains(&candidate.short_ref) {
                return Err(AppError::Domain {
                    code: "MEMORY_COVERAGE_INCOMPLETE".to_string(),
                    message: format!(
                        "Candidate session {} (short ref {}) not covered in generation result",
                        candidate.session_id, candidate.short_ref
                    ),
                    retryable: true,
                    details: None,
                });
            }
        }

        if covered_set.len() != candidates.len() {
            return Err(AppError::Domain {
                code: "MEMORY_COVERAGE_INCOMPLETE".to_string(),
                message: "Generation coverage contains more or fewer sessions than the frozen candidate set".to_string(),
                retryable: true,
                details: None,
            });
        }

        // 2. Project & Item 校验
        let candidate_project_keys = candidates
            .iter()
            .map(|c| c.project_key.clone())
            .collect::<HashSet<_>>();

        let mut result_project_keys = HashSet::new();
        for project in &result.projects {
            let key = project.project_key.trim();
            if key.is_empty() {
                return Err(AppError::Validation(
                    "Project key in memory generation result cannot be empty".to_string(),
                ));
            }

            // 项目 key 必须来自候选集或为 unassigned
            if !candidate_project_keys.contains(key)
                && key != "unassigned"
                && !candidates.is_empty()
            {
                return Err(AppError::Validation(format!(
                    "Project key '{}' not present in candidate work order",
                    key
                )));
            }
            if !result_project_keys.insert(key.to_string()) {
                return Err(AppError::Validation(format!(
                    "Duplicate project '{}' in memory generation result",
                    key
                )));
            }

            for session_ref in &project.source_sessions {
                let candidate = candidate_by_ref.get(session_ref.as_str()).ok_or_else(|| {
                    AppError::Validation(format!(
                        "Unknown source session reference '{}' in project '{}'",
                        session_ref, key
                    ))
                })?;
                if candidate.project_key != key {
                    return Err(AppError::Validation(format!(
                        "Project '{}' references session '{}' from project '{}'",
                        key, session_ref, candidate.project_key
                    )));
                }
            }

            // 摘要长度
            if !project.no_material_change
                && (project.summary.trim().is_empty() || project.summary.len() > 2000)
            {
                return Err(AppError::Validation(format!(
                    "Project '{}' summary must be 1..2000 chars when no_material_change is false",
                    key
                )));
            }

            // 建议排序 (0..3 个 recommendation_rank, 1..=3 且不重复)
            let mut rank_set = HashSet::new();
            for item in &project.items {
                if let Some(rank) = item.recommendation_rank {
                    if !(1..=3).contains(&rank) {
                        return Err(AppError::Validation(format!(
                            "Recommendation rank {} in project '{}' is out of range 1..=3",
                            rank, key
                        )));
                    }
                    if !rank_set.insert(rank) {
                        return Err(AppError::Validation(format!(
                            "Duplicate recommendation rank {} in project '{}'",
                            rank, key
                        )));
                    }
                    if item.source_refs.is_empty() {
                        return Err(AppError::Validation(format!(
                            "Recommendation item '{}' in project '{}' must have at least one source ref",
                            item.title, key
                        )));
                    }
                }

                // 字段长度
                let title = item.title.trim();
                if title.is_empty() || title.len() > 200 {
                    return Err(AppError::Validation(format!(
                        "Item title must be 1..200 chars, got length {}",
                        title.len()
                    )));
                }
                if item.summary.trim().is_empty() || item.summary.len() > 2000 {
                    return Err(AppError::Validation(format!(
                        "Item summary must be 1..2000 chars, got length {}",
                        item.summary.len()
                    )));
                }
                if item.rationale.trim().is_empty() || item.rationale.len() > 2000 {
                    return Err(AppError::Validation(format!(
                        "Item rationale must be 1..2000 chars, got length {}",
                        item.rationale.len()
                    )));
                }

                // 引用校验: 所有 source_refs 必须合法在 ref_map 中存在
                for ref_key in &item.source_refs {
                    let resolved = ref_map.get(ref_key).ok_or_else(|| {
                        AppError::Validation(format!(
                            "Unknown source reference '{}' in item '{}'",
                            ref_key, item.title
                        ))
                    })?;
                    if resolved.project_key != key {
                        return Err(AppError::Validation(format!(
                            "Item '{}' in project '{}' references session '{}' from project '{}'",
                            item.title, key, ref_key, resolved.project_key
                        )));
                    }
                }

                if item.promotion_nomination != MemoryPromotionNomination::None
                    && item.source_refs.is_empty()
                {
                    return Err(AppError::Validation(format!(
                        "Promoted item '{}' must have at least one source ref",
                        item.title
                    )));
                }

                // M35-L1-12: unassigned 项目不能被提名晋升 L2
                if key == "unassigned"
                    && item.promotion_nomination != MemoryPromotionNomination::None
                {
                    return Err(AppError::Validation(
                        "Items in unassigned project cannot be nominated for promotion".to_string(),
                    ));
                }
            }

            if rank_set.len() > 3 {
                return Err(AppError::Validation(format!(
                    "Project '{}' exceeds maximum 3 recommendations",
                    key
                )));
            }
        }

        if result_project_keys != candidate_project_keys {
            return Err(AppError::Domain {
                code: "MEMORY_COVERAGE_INCOMPLETE".to_string(),
                message:
                    "Every candidate project must have exactly one Recent Snapshot project output"
                        .to_string(),
                retryable: true,
                details: None,
            });
        }

        Ok(())
    }
}
