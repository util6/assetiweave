use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row, Sqlite};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};
pub(crate) fn extract_json_payload(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let Some(start) = trimmed.find("```json") {
        let after_start = &trimmed[start + 7..];
        if let Some(end) = after_start.rfind("```") {
            return after_start[..end].trim();
        }
    } else if let Some(start) = trimmed.find("```") {
        let after_start = &trimmed[start + 3..];
        if let Some(end) = after_start.rfind("```") {
            return after_start[..end].trim();
        }
    }
    if let Some(first_brace) = trimmed.find('{') {
        if let Some(last_brace) = trimmed.rfind('}') {
            if last_brace > first_brace {
                return trimmed[first_brace..=last_brace].trim();
            }
        }
    }
    trimmed
}

pub(crate) fn parse_memory_generation_output(raw: &str) -> AppResult<MemoryGenerationResult> {
    let json_text = extract_json_payload(raw);
    if json_text.is_empty() {
        return Err(AppError::Validation(
            "MEMORY_OUTPUT_INVALID: empty Agent output".to_string(),
        ));
    }
    match serde_json::from_str::<MemoryGenerationResult>(json_text) {
        Ok(result) => Ok(result),
        Err(orig_err) => {
            if let Some(repaired) = attempt_repair_truncated_json(json_text) {
                if let Ok(result) = serde_json::from_str::<MemoryGenerationResult>(&repaired) {
                    tracing::warn!(
                        "Successfully repaired truncated JSON in memory generation output"
                    );
                    return Ok(result);
                }
            }
            Err(AppError::Validation(format!(
                "MEMORY_OUTPUT_INVALID: expected one MemoryGenerationResult JSON value: {orig_err}"
            )))
        }
    }
}

pub(crate) fn attempt_repair_truncated_json(input: &str) -> Option<String> {
    let s = input.trim();
    if !s.starts_with('{') {
        return None;
    }

    let mut last_brace = s.rfind('}')?;
    while last_brace > 0 {
        let candidate = &s[..=last_brace];
        let mut stack = Vec::new();
        let mut in_string = false;
        let mut escape = false;
        let mut valid = true;

        for ch in candidate.chars() {
            if in_string {
                if escape {
                    escape = false;
                } else if ch == '\\' {
                    escape = true;
                } else if ch == '"' {
                    in_string = false;
                }
            } else {
                match ch {
                    '"' => in_string = true,
                    '{' => stack.push('}'),
                    '[' => stack.push(']'),
                    '}' | ']' => {
                        if stack.pop() != Some(ch) {
                            valid = false;
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }

        if valid && !in_string {
            let mut repaired = candidate.to_string();
            while let Some(closing) = stack.pop() {
                repaired.push(closing);
            }
            return Some(repaired);
        }

        last_brace = s[..last_brace].rfind('}')?;
    }
    None
}
