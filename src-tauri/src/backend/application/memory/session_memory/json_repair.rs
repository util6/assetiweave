use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};
pub(crate) fn strip_json_fence(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return trimmed;
    }

    // Agent 偶尔会在 JSON 字符串值中直接输出未转义的引号。此时外层对象
    // 虽然暂时无法被 serde_json 解析，结构边界仍然是完整的。必须优先保留
    // 这个外层对象，不能继续向内扫描并把某个 Recent Event 子对象误当成
    // 整份 Session Memory。
    if trimmed.starts_with('{') {
        if let Some(candidate) = json_object_from_start(trimmed) {
            if is_parseable_or_repairable_memory_candidate(candidate) {
                return candidate;
            }
        }
    }

    // 1. 如果整体已经是 Session Memory JSON 对象，直接返回
    if first_valid_json_object(trimmed).is_some_and(|candidate| candidate == trimmed) {
        return trimmed;
    }

    // 2. 扫描所有代码围栏 (```json 或 ```)，优先提取第一个能解析成合法 JSON 对象的块
    let mut search_pos = 0;
    while let Some(start_rel) = trimmed[search_pos..].find("```") {
        let block_start = search_pos + start_rel;
        let content_start = if trimmed[block_start..].starts_with("```json") {
            block_start + 7
        } else {
            block_start + 3
        };
        if let Some(end_rel) = trimmed[content_start..].find("```") {
            let block_end = content_start + end_rel;
            let block_content = trimmed[content_start..block_end].trim();
            if block_content.starts_with('{') {
                if let Some(candidate) = json_object_from_start(block_content) {
                    if is_parseable_or_repairable_memory_candidate(candidate) {
                        return candidate;
                    }
                }
            }
            if let Some(candidate) = first_session_memory_json_object(block_content)
                .or_else(|| first_valid_json_object(block_content))
            {
                return candidate;
            }
            search_pos = block_end + 3;
        } else {
            break;
        }
    }

    // 3. 从前往后扫描平衡的大括号，避免前置思考文本中的无关 `{...}`
    // 把真正的业务 JSON 与尾部内容拼成一个不可解析的大区间。
    if let Some(candidate) =
        first_session_memory_json_object(trimmed).or_else(|| first_valid_json_object(trimmed))
    {
        return candidate;
    }

    trimmed
}

pub(crate) fn parse_session_memory_agent_output(
    value: &str,
) -> Result<SessionMemoryAgentOutput, serde_json::Error> {
    let candidate = strip_json_fence(value);
    match serde_json::from_str(candidate) {
        Ok(output) => Ok(output),
        Err(original_error) => {
            let repaired = repair_unescaped_json_string_quotes(candidate);
            if repaired == candidate {
                return Err(original_error);
            }
            serde_json::from_str(&repaired).map_err(|_| original_error)
        }
    }
}

/// Repairs the narrow, common model-output defect where a quote inside a JSON
/// string was emitted without a backslash. A quote is treated as the end of a
/// JSON string only when the next non-whitespace character is a legal
/// structural delimiter. The repaired text is still deserialized into the
/// typed contract afterwards, so this does not admit invalid field shapes.
pub(crate) fn repair_unescaped_json_string_quotes(value: &str) -> String {
    let mut repaired = String::with_capacity(value.len());
    let mut in_string = false;
    let mut escaped = false;
    let characters = value.char_indices().collect::<Vec<_>>();

    for (index, (byte_offset, character)) in characters.iter().copied().enumerate() {
        if !in_string {
            repaired.push(character);
            if character == '"' {
                in_string = true;
            }
            continue;
        }

        if escaped {
            repaired.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' {
            repaired.push(character);
            escaped = true;
            continue;
        }
        if character != '"' {
            repaired.push(character);
            continue;
        }

        let next_non_whitespace = characters[index + 1..]
            .iter()
            .map(|(_, next)| *next)
            .find(|next| !next.is_whitespace());
        if next_non_whitespace.is_none_or(|next| matches!(next, ':' | ',' | '}' | ']')) {
            repaired.push(character);
            in_string = false;
        } else {
            repaired.push('\\');
            repaired.push(character);
        }

        debug_assert!(value.is_char_boundary(byte_offset));
    }

    repaired
}

pub(crate) fn is_parseable_or_repairable_memory_candidate(value: &str) -> bool {
    if serde_json::from_str::<serde_json::Map<String, Value>>(value).is_ok() {
        return true;
    }
    let repaired = repair_unescaped_json_string_quotes(value);
    serde_json::from_str::<serde_json::Map<String, Value>>(&repaired)
        .is_ok_and(|object| is_session_memory_json_object(&object))
}

pub(crate) fn json_object_from_start(value: &str) -> Option<&str> {
    if !value.starts_with('{') {
        return None;
    }
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, current) in value.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if current == '\\' {
                escaped = true;
            } else if current == '"' {
                in_string = false;
            }
            continue;
        }
        match current {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(&value[..offset + current.len_utf8()]);
                }
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn first_valid_json_object(value: &str) -> Option<&str> {
    first_json_object_matching(value, |_| true)
}

pub(crate) fn first_session_memory_json_object(value: &str) -> Option<&str> {
    first_json_object_matching(value, is_session_memory_json_object)
}

pub(crate) fn is_session_memory_json_object(object: &serde_json::Map<String, Value>) -> bool {
    let has_memory_field = object.keys().any(|key| {
        matches!(
            key.as_str(),
            "goal"
                | "result"
                | "decisions"
                | "verification"
                | "blockers"
                | "follow_up"
                | "followUp"
                | "topics"
                | "source_references"
                | "sourceReferences"
                | "events"
                | "recentEvents"
        )
    });
    let is_standalone_summary = object.contains_key("summary")
        && !object.contains_key("category")
        && !object.contains_key("title")
        && !object.contains_key("source_reference")
        && !object.contains_key("sourceReference");
    has_memory_field || is_standalone_summary
}

pub(crate) fn first_json_object_matching(
    value: &str,
    predicate: impl Fn(&serde_json::Map<String, Value>) -> bool,
) -> Option<&str> {
    for (start, character) in value.char_indices() {
        if character != '{' {
            continue;
        }
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        for (offset, current) in value[start..].char_indices() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if current == '\\' {
                    escaped = true;
                } else if current == '"' {
                    in_string = false;
                }
                continue;
            }
            match current {
                '"' => in_string = true,
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let end = start + offset + current.len_utf8();
                        let candidate = &value[start..end];
                        if serde_json::from_str::<serde_json::Map<String, Value>>(candidate)
                            .is_ok_and(|object| predicate(&object))
                        {
                            return Some(candidate);
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    None
}

pub(crate) fn digest(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}
