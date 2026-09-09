use std::collections::{HashMap, HashSet};

use crate::backend::evidence::types::{
    BoundedEvidenceNode, EvidenceIndexEntry, EvidenceNodeKind, EvidenceReadError,
    EvidenceReadStatus, ShortEvidenceRef,
};
use crate::backend::memory_redaction::redact_memory_text;
use crate::backend::models::{MemoryExecutionWorkOrder, NormalizedConversationSession};

pub struct BoundedEvidenceReaderSession<'a> {
    pub session: &'a NormalizedConversationSession,
    pub work_order: &'a MemoryExecutionWorkOrder,
    pub short_refs: HashMap<String, ShortEvidenceRef>,
    pub cumulative_tool_chars: usize,
    pub tool_call_count: usize,
    pub budget_exhausted: bool,
    pub read_node_keys: HashSet<String>,
}

impl<'a> BoundedEvidenceReaderSession<'a> {
    pub fn new(
        session: &'a NormalizedConversationSession,
        work_order: &'a MemoryExecutionWorkOrder,
        short_refs: HashMap<String, ShortEvidenceRef>,
    ) -> Self {
        Self {
            session,
            work_order,
            short_refs,
            cumulative_tool_chars: 0,
            tool_call_count: 0,
            budget_exhausted: false,
            read_node_keys: HashSet::new(),
        }
    }

    pub fn check_tool_permission_and_budget(
        &mut self,
        tool_name: &str,
    ) -> Result<(), EvidenceReadError> {
        if !self.work_order.is_allowed_tool(tool_name) {
            return Err(EvidenceReadError::UnauthorizedTool(tool_name.to_string()));
        }

        if self.budget_exhausted {
            return Err(EvidenceReadError::BudgetExhausted {
                reason: "Budget policy already exhausted in this session".to_string(),
            });
        }

        let budget = &self.work_order.budget_policy;
        if self.tool_call_count >= budget.tool_call_limit {
            self.budget_exhausted = true;
            return Err(EvidenceReadError::BudgetExhausted {
                reason: format!(
                    "Tool call count reached limit of {}",
                    budget.tool_call_limit
                ),
            });
        }

        self.tool_call_count += 1;
        Ok(())
    }

    fn apply_budget_to_output(&mut self, text: String) -> Result<String, EvidenceReadError> {
        let budget = &self.work_order.budget_policy;

        // 单次工具响应截断
        let single_limit = budget.tool_response_max_chars;
        let response_text = if text.chars().count() > single_limit {
            text.chars().take(single_limit).collect::<String>()
        } else {
            text
        };

        let response_len = response_text.len();
        if self.cumulative_tool_chars + response_len > budget.tool_cumulative_max_chars {
            self.budget_exhausted = true;
            let available = budget
                .tool_cumulative_max_chars
                .saturating_sub(self.cumulative_tool_chars);
            let truncated: String = response_text.chars().take(available).collect();
            self.cumulative_tool_chars = budget.tool_cumulative_max_chars;
            return Ok(truncated);
        }

        self.cumulative_tool_chars += response_len;
        Ok(response_text)
    }

    /// 工具 1: get_session_outline
    pub fn get_session_outline(&mut self) -> Result<String, EvidenceReadError> {
        self.check_tool_permission_and_budget("get_session_outline")?;

        let mut outline = format!("Session ID: {}\n", self.session.external_id);
        for (t_idx, turn) in self.session.turns.iter().enumerate() {
            outline.push_str(&format!(
                "Turn {} (index: {}, id: {}): user_text_len={}, parts={}\n",
                t_idx + 1,
                turn.turn_index,
                turn.external_id,
                turn.user_text.len(),
                turn.parts.len()
            ));
            if !turn.user_text.trim().is_empty() {
                let user_ref = format!("ref-t{}-u", t_idx + 1);
                outline.push_str(&format!("  User Ref: {}\n", user_ref));
            }
            for (p_idx, part) in turn.parts.iter().enumerate() {
                let ref_key = format!("ref-t{}-p{}", t_idx + 1, p_idx + 1);
                outline.push_str(&format!(
                    "  Part Ref: {} [{}]\n",
                    ref_key,
                    part.role.as_str()
                ));
            }
        }

        self.apply_budget_to_output(outline)
    }

    /// 工具 2: search_session_content
    pub fn search_session_content(
        &mut self,
        query: &str,
    ) -> Result<Vec<EvidenceIndexEntry>, EvidenceReadError> {
        self.check_tool_permission_and_budget("search_session_content")?;

        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for (t_idx, turn) in self.session.turns.iter().enumerate() {
            if turn.user_text.to_lowercase().contains(&query_lower) {
                let ref_key = format!("ref-t{}-u", t_idx + 1);
                let snippet: String = turn.user_text.chars().take(150).collect();
                results.push(EvidenceIndexEntry {
                    ref_key,
                    turn_id: turn.external_id.clone(),
                    turn_index: turn.turn_index,
                    role: "user".to_string(),
                    snippet,
                    total_chars: turn.user_text.len(),
                    has_more: turn.user_text.chars().count() > 150,
                });
            }

            for (p_idx, part) in turn.parts.iter().enumerate() {
                let ref_key = format!("ref-t{}-p{}", t_idx + 1, p_idx + 1);
                let raw_text = part.text.as_deref().unwrap_or("").trim();
                if raw_text.to_lowercase().contains(&query_lower) {
                    let snippet: String = raw_text.chars().take(150).collect();
                    results.push(EvidenceIndexEntry {
                        ref_key,
                        turn_id: turn.external_id.clone(),
                        turn_index: turn.turn_index,
                        role: part.role.as_str().to_string(),
                        snippet,
                        total_chars: raw_text.len(),
                        has_more: raw_text.chars().count() > 150,
                    });
                }
            }
        }

        let serialized = serde_json::to_string(&results).unwrap_or_default();
        let _ = self.apply_budget_to_output(serialized)?;

        Ok(results)
    }

    /// 工具 3: read_question_content (读取特定 Turn 的内容)
    pub fn read_question_content(
        &mut self,
        turn_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<String, EvidenceReadError> {
        self.check_tool_permission_and_budget("read_question_content")?;

        let turn = self
            .session
            .turns
            .iter()
            .find(|t| t.external_id == turn_id)
            .ok_or_else(|| EvidenceReadError::OutOfScope {
                requested: turn_id.to_string(),
                reason: "Turn ID not found in current session".to_string(),
            })?;

        let mut text = String::new();
        if !turn.user_text.trim().is_empty() {
            text.push_str("User: ");
            text.push_str(&turn.user_text);
            text.push('\n');
        }
        for part in &turn.parts {
            if let Some(t) = &part.text {
                text.push_str(&format!("{}: {}\n", part.role.as_str(), t));
            }
        }

        let redacted = redact_memory_text(&text).text;
        let chars_count = redacted.chars().count();
        let slice: String = redacted.chars().skip(offset).take(limit).collect();
        let result = format!("[Offset: {}, Total: {}]\n{}", offset, chars_count, slice);

        self.apply_budget_to_output(result)
    }

    /// 工具 4: read_content_node (按 short_ref 读节点正文)
    pub fn read_content_node(
        &mut self,
        ref_key: &str,
    ) -> Result<BoundedEvidenceNode, EvidenceReadError> {
        self.check_tool_permission_and_budget("read_content_node")?;

        let (turn_id, part_index, is_user) = {
            let short_ref =
                self.short_refs
                    .get(ref_key)
                    .ok_or_else(|| EvidenceReadError::OutOfScope {
                        requested: ref_key.to_string(),
                        reason: "Short reference key not found in current session mapping"
                            .to_string(),
                    })?;

            if short_ref.status == EvidenceReadStatus::Unavailable {
                return Err(EvidenceReadError::ContentUnavailable {
                    ref_key: ref_key.to_string(),
                });
            }

            (
                short_ref.turn_id.clone(),
                short_ref.part_index,
                short_ref.ref_key.ends_with("-u"),
            )
        };

        let turn = self
            .session
            .turns
            .iter()
            .find(|t| t.external_id == turn_id)
            .ok_or_else(|| EvidenceReadError::OutOfScope {
                requested: ref_key.to_string(),
                reason: "Turn ID not found in session".to_string(),
            })?;

        let (role, raw_text) = if is_user {
            ("user".to_string(), turn.user_text.clone())
        } else {
            let part = turn
                .parts
                .get(part_index)
                .ok_or_else(|| EvidenceReadError::OutOfScope {
                    requested: ref_key.to_string(),
                    reason: "Part index out of bounds".to_string(),
                })?;
            (
                part.role.as_str().to_string(),
                part.text.as_deref().unwrap_or("").to_string(),
            )
        };

        let raw_text_trimmed = raw_text.trim();
        if raw_text_trimmed.is_empty() {
            return Err(EvidenceReadError::ContentUnavailable {
                ref_key: ref_key.to_string(),
            });
        }

        let redacted = redact_memory_text(raw_text_trimmed).text;
        let budget = &self.work_order.budget_policy;

        let (final_text, truncated) = if redacted.chars().count() > budget.single_item_max_chars {
            (
                redacted
                    .chars()
                    .take(budget.single_item_max_chars)
                    .collect::<String>(),
                true,
            )
        } else {
            (redacted, false)
        };

        let bounded_text = self.apply_budget_to_output(final_text)?;
        if let Some(short_ref) = self.short_refs.get_mut(ref_key) {
            short_ref.status = EvidenceReadStatus::ReadByTool;
        }
        self.read_node_keys.insert(ref_key.to_string());

        Ok(BoundedEvidenceNode {
            ref_key: ref_key.to_string(),
            role,
            kind: EvidenceNodeKind::ExecutionEvidence,
            title: format!("Ref {}", ref_key),
            text: bounded_text,
            truncated,
        })
    }
}
