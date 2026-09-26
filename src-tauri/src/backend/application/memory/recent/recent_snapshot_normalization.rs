use std::collections::{HashMap, HashSet};

use crate::backend::domain::{
    CandidateSession, MemoryGenerationProject, MemoryGenerationResult, MemoryItemCategory,
    MemoryPromotionNomination, ResolvedEvidenceRef,
};

/// 规范化 Agent 返回的记忆生成结果，修正模型可能产生的非致命偏差 (Schema, Coverage, Project/Item Refs)
pub(crate) fn normalize_agent_memory_generation_result(
    result: &mut MemoryGenerationResult,
    candidates: &[CandidateSession],
    ref_map: &HashMap<String, ResolvedEvidenceRef>,
) {
    normalize_projects_and_items(result, candidates, ref_map);
    normalize_coverage(result, candidates, ref_map);
}

fn normalize_projects_and_items(
    result: &mut MemoryGenerationResult,
    candidates: &[CandidateSession],
    ref_map: &HashMap<String, ResolvedEvidenceRef>,
) {
    let candidate_by_ref = candidates
        .iter()
        .map(|c| (c.short_ref.as_str(), c))
        .collect::<HashMap<_, _>>();
    let candidate_project_keys = candidates
        .iter()
        .map(|c| c.project_key.as_str())
        .collect::<HashSet<_>>();

    let mut seen_keys = HashSet::new();
    result.projects.retain_mut(|project| {
        let key = project.project_key.trim();
        if key.is_empty() {
            return false;
        }
        if !candidates.is_empty() && !candidate_project_keys.contains(key) {
            return false;
        }
        seen_keys.insert(key.to_string())
    });

    for project_key in &candidate_project_keys {
        if !seen_keys.contains(*project_key) {
            let project_sessions = candidates
                .iter()
                .filter(|c| c.project_key == *project_key)
                .map(|c| c.short_ref.clone())
                .collect::<Vec<_>>();
            result.projects.push(MemoryGenerationProject {
                project_key: (*project_key).to_string(),
                summary: "No material change in this window.".to_string(),
                no_material_change: true,
                source_sessions: project_sessions,
                items: Vec::new(),
            });
            seen_keys.insert((*project_key).to_string());
        }
    }

    for project in &mut result.projects {
        let project_key = project.project_key.trim().to_string();
        project.project_key = project_key.clone();

        // 仅保留属于当前 project 的 session_ref
        project.source_sessions.retain(|session_ref| {
            candidate_by_ref
                .get(session_ref.trim())
                .map_or(false, |c| c.project_key == project_key)
        });

        if project.source_sessions.is_empty() {
            for c in candidates {
                if c.project_key == project_key {
                    project.source_sessions.push(c.short_ref.clone());
                }
            }
        }

        let project_available_refs = ref_map
            .iter()
            .filter(|(_, res)| res.project_key == project_key)
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>();

        let mut rank_set = HashSet::new();
        for item in &mut project.items {
            item.source_refs.retain(|ref_key| {
                ref_map
                    .get(ref_key.trim())
                    .map_or(false, |res| res.project_key == project_key)
            });

            if item.source_refs.is_empty() && !project_available_refs.is_empty() {
                item.source_refs.push(project_available_refs[0].clone());
            }

            if let Some(rank) = item.recommendation_rank {
                if (1..=3).contains(&rank) && !item.source_refs.is_empty() && rank_set.insert(rank)
                {
                    // 保留有效建议
                } else {
                    item.recommendation_rank = None;
                }
            }

            if project_key == "unassigned" || item.source_refs.is_empty() {
                item.promotion_nomination = MemoryPromotionNomination::None;
            }
        }

        if rank_set.len() < 3 && !project_available_refs.is_empty() {
            let mut available_ranks = Vec::new();
            for r in (1..=3).rev() {
                if !rank_set.contains(&r) {
                    available_ranks.push(r);
                }
            }
            for item in &mut project.items {
                if item.recommendation_rank.is_some() {
                    continue;
                }
                let is_actionable = matches!(
                    item.category,
                    MemoryItemCategory::FollowUp | MemoryItemCategory::Blocker
                );
                if is_actionable {
                    if let Some(next_rank) = available_ranks.pop() {
                        item.recommendation_rank = Some(next_rank);
                        rank_set.insert(next_rank);
                        if item.source_refs.is_empty() {
                            item.source_refs.push(project_available_refs[0].clone());
                        }
                    }
                }
                if available_ranks.is_empty() {
                    break;
                }
            }
        }
    }
}

fn normalize_coverage(
    result: &mut MemoryGenerationResult,
    candidates: &[CandidateSession],
    ref_map: &HashMap<String, ResolvedEvidenceRef>,
) {
    if candidates.is_empty() {
        result.coverage.covered_sessions.clear();
        result.coverage.no_memory_sessions.clear();
        return;
    }

    // 构建各种可能引用的别名映射到 canonical short_ref (如 s1, S1, sessionId 等)
    let mut alias_to_canonical = HashMap::new();
    for c in candidates {
        alias_to_canonical.insert(c.short_ref.clone(), c.short_ref.clone());
        alias_to_canonical.insert(c.short_ref.to_ascii_lowercase(), c.short_ref.clone());
        alias_to_canonical.insert(c.session_id.clone(), c.short_ref.clone());
        alias_to_canonical.insert(
            format!("{}/{}", c.source_id, c.session_id),
            c.short_ref.clone(),
        );
    }

    // 找出所有确实产生了记忆卡片 (items) 的 candidate session
    let mut sessions_with_items = HashSet::new();
    for project in &result.projects {
        for item in &project.items {
            for ref_key in &item.source_refs {
                let trimmed_ref = ref_key.trim();
                if let Some(resolved) = ref_map.get(trimmed_ref) {
                    if let Some(canonical) = alias_to_canonical.get(&resolved.session_id) {
                        sessions_with_items.insert(canonical.clone());
                    }
                } else if let Some(prefix) = trimmed_ref.split('.').next() {
                    if let Some(canonical) = alias_to_canonical.get(prefix) {
                        sessions_with_items.insert(canonical.clone());
                    }
                }
            }
        }
    }

    let mut covered_set = HashSet::new();
    let mut normalized_covered = Vec::new();

    // 1. 凡是有记忆卡片产出的候选 Session，必定属于 covered_sessions
    for candidate in candidates {
        if sessions_with_items.contains(&candidate.short_ref) {
            if covered_set.insert(candidate.short_ref.clone()) {
                normalized_covered.push(candidate.short_ref.clone());
            }
        }
    }

    // 2. 处理模型显式指明的 no_memory_sessions
    // 注意：若某 Session 产生过 memory item，绝不可归入 no_memory_sessions
    let mut no_memory_set = HashSet::new();
    let mut normalized_no_memory = Vec::new();

    for raw_ref in &result.coverage.no_memory_sessions {
        let trimmed = raw_ref.trim();
        if let Some(canonical) = alias_to_canonical
            .get(trimmed)
            .or_else(|| alias_to_canonical.get(&trimmed.to_ascii_lowercase()))
        {
            if !sessions_with_items.contains(canonical) {
                if no_memory_set.insert(canonical.clone()) {
                    normalized_no_memory.push(canonical.clone());
                }
            }
        }
    }

    // 3. 处理模型显式指明的 covered_sessions
    // 注意：若已被归入 no_memory_set 且无记忆卡片，则不重复放入 covered_set
    for raw_ref in &result.coverage.covered_sessions {
        let trimmed = raw_ref.trim();
        if let Some(canonical) = alias_to_canonical
            .get(trimmed)
            .or_else(|| alias_to_canonical.get(&trimmed.to_ascii_lowercase()))
        {
            if !no_memory_set.contains(canonical) {
                if covered_set.insert(canonical.clone()) {
                    normalized_covered.push(canonical.clone());
                }
            }
        }
    }

    // 4. 对未出现在任何集合中的候选 Session 进行保底划分，保证 coverage 完整性
    for candidate in candidates {
        if !covered_set.contains(&candidate.short_ref)
            && !no_memory_set.contains(&candidate.short_ref)
        {
            let project_has_items = result
                .projects
                .iter()
                .any(|p| p.project_key == candidate.project_key && !p.items.is_empty());
            if project_has_items {
                covered_set.insert(candidate.short_ref.clone());
                normalized_covered.push(candidate.short_ref.clone());
            } else {
                no_memory_set.insert(candidate.short_ref.clone());
                normalized_no_memory.push(candidate.short_ref.clone());
            }
        }
    }

    // 5. 过滤 unreadable_sessions 中的未知别名
    result.coverage.unreadable_sessions.retain_mut(|s| {
        let trimmed = s.trim().to_string();
        *s = trimmed.clone();
        alias_to_canonical.contains_key(&trimmed)
            || alias_to_canonical.contains_key(&trimmed.to_ascii_lowercase())
    });

    result.coverage.covered_sessions = normalized_covered;
    result.coverage.no_memory_sessions = normalized_no_memory;
}

#[cfg(test)]
#[path = "recent_snapshot_normalization_tests.rs"]
mod tests;
