use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

use serde_json::Value;

use crate::backend::{
    agents::types::{AgentDefinition, AgentEnvEntry},
    ai_execution::PersistentExecutionBinding,
};

use super::history_replay::{
    HistoryReplayEntry, HistoryReplayFidelity, HistoryReplayFuture, HistoryReplayPort,
    HistoryReplayResult, HistoryReplayStatus,
};

const PROVIDER_ROOT_ENV: &str = "ASSETIWEAVE_ANTIGRAVITY_PROVIDER_ROOT";
const PROVIDER_ROOT_METADATA_KEYS: [&str; 2] = ["provider_store_root", "history_root"];
const FULL_TRANSCRIPT: &str = "transcript_full.jsonl";
const SIMPLIFIED_TRANSCRIPT: &str = "transcript.jsonl";

/// Read-only access to Antigravity's Provider-owned conversation store.
///
/// The root is resolved from binding metadata first, then from the Agent's
/// launch environment and finally from the normal Antigravity home. Tests pass
/// a temporary root in binding metadata; no test depends on a user's account or
/// home directory.
pub(crate) struct AntigravityProviderHistoryReader {
    root: Option<PathBuf>,
}

impl AntigravityProviderHistoryReader {
    pub(crate) fn from_request(
        definition: &AgentDefinition,
        binding: Option<&PersistentExecutionBinding>,
    ) -> Self {
        let root = binding
            .and_then(metadata_root)
            .or_else(|| env_entry(&definition.env, PROVIDER_ROOT_ENV))
            .or_else(|| std::env::var_os(PROVIDER_ROOT_ENV).map(PathBuf::from))
            .or_else(default_provider_root);
        Self { root }
    }

    #[cfg(test)]
    pub(crate) fn from_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: Some(root.into()),
        }
    }

    fn conversation_dirs(&self, anchor: &str) -> Vec<PathBuf> {
        let Some(root) = self.root.as_deref() else {
            return Vec::new();
        };
        if !valid_anchor_component(anchor) {
            return Vec::new();
        }

        let mut candidates = Vec::new();
        let mut add = |path: PathBuf| {
            if !candidates.iter().any(|candidate| candidate == &path) {
                candidates.push(path);
            }
        };

        // Accept a direct `brain` root as well as a provider home containing
        // `brain` and sibling IDE/CLI environments.
        add(root.join(anchor));
        add(root.join("brain").join(anchor));
        if root.file_name().is_some_and(|name| name == "brain") {
            add(root.join(anchor));
        }

        let mut children = fs::read_dir(root)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_dir()))
            .collect::<Vec<_>>();
        children.sort_by_key(|entry| entry.file_name());
        for child in children {
            let child_path = child.path();
            add(child_path.join("brain").join(anchor));
            add(child_path.join(anchor));
        }
        candidates
    }

    fn read_candidate(
        &self,
        conversation_dir: &Path,
        max_bytes: usize,
    ) -> Option<HistoryReplayResult> {
        let logs = conversation_dir.join(".system_generated").join("logs");
        let full_path = logs.join(FULL_TRANSCRIPT);
        let short_path = logs.join(SIMPLIFIED_TRANSCRIPT);

        if full_path.is_file() {
            if let Some(full) = read_transcript(&full_path, HistoryReplayFidelity::Full, max_bytes)
            {
                if has_visible_entries(&full) || !short_path.is_file() {
                    return Some(full);
                }
                if let Some(short) =
                    read_transcript(&short_path, HistoryReplayFidelity::Simplified, max_bytes)
                {
                    return Some(degraded_fallback(short));
                }
                return Some(full);
            }
        }
        if short_path.is_file() {
            return read_transcript(&short_path, HistoryReplayFidelity::Simplified, max_bytes);
        }
        None
    }
}

impl AntigravityProviderHistoryReader {
    fn replay_sync(&self, provider_session_id: &str, max_bytes: usize) -> HistoryReplayResult {
        if !valid_anchor_component(provider_session_id) {
            return HistoryReplayResult::unavailable();
        }
        self.conversation_dirs(provider_session_id)
            .into_iter()
            .find_map(|conversation_dir| self.read_candidate(&conversation_dir, max_bytes))
            .unwrap_or_else(HistoryReplayResult::unavailable)
    }
}

impl HistoryReplayPort for AntigravityProviderHistoryReader {
    fn replay<'a>(
        &'a mut self,
        provider_session_id: &'a str,
        max_bytes: usize,
    ) -> HistoryReplayFuture<'a> {
        let reader = Self {
            root: self.root.clone(),
        };
        let provider_session_id = provider_session_id.to_owned();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || reader.replay_sync(&provider_session_id, max_bytes))
                .await
                .unwrap_or_else(|_| HistoryReplayResult::unavailable())
        })
    }
}

fn metadata_root(binding: &PersistentExecutionBinding) -> Option<PathBuf> {
    let value = serde_json::from_str::<Value>(&binding.provider_metadata_json).ok()?;
    PROVIDER_ROOT_METADATA_KEYS
        .iter()
        .find_map(|key| value.get(key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

fn env_entry(entries: &[AgentEnvEntry], name: &str) -> Option<PathBuf> {
    entries
        .iter()
        .find(|entry| entry.name == name)
        .map(|entry| entry.value.trim())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn default_provider_root() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".gemini"))
}

fn valid_anchor_component(anchor: &str) -> bool {
    let anchor = anchor.trim();
    !anchor.is_empty()
        && anchor.len() <= 256
        && anchor != "."
        && anchor != ".."
        && !anchor.contains(['/', '\\'])
        && !anchor.chars().any(char::is_control)
}

struct ParsedTranscript {
    entries: Vec<OrderedEntry>,
    malformed: bool,
    truncated: bool,
}

struct OrderedEntry {
    step_index: u64,
    source_index: usize,
    sub_index: usize,
    entry: HistoryReplayEntry,
}

fn read_transcript(
    path: &Path,
    source_fidelity: HistoryReplayFidelity,
    max_bytes: usize,
) -> Option<HistoryReplayResult> {
    let (bytes, truncated) = read_bounded(path, max_bytes).ok()?;
    let parsed = parse_transcript(&bytes, truncated);
    let mut entries = parsed.entries;
    entries.sort_by_key(|entry| (entry.step_index, entry.source_index, entry.sub_index));

    let mut text_parts = Vec::new();
    let mut replay_entries = Vec::with_capacity(entries.len() + 1);
    for ordered in entries {
        if let HistoryReplayEntry::AssistantText { text } = &ordered.entry {
            text_parts.push(text.clone());
        }
        replay_entries.push(ordered.entry);
    }

    let visible = replay_entries
        .iter()
        .any(|entry| !matches!(entry, HistoryReplayEntry::Notice { .. }));
    let damaged = parsed.malformed || parsed.truncated;
    let (fidelity, status) = if !visible {
        (
            HistoryReplayFidelity::Unavailable,
            HistoryReplayStatus::Unavailable,
        )
    } else if damaged {
        (HistoryReplayFidelity::Partial, HistoryReplayStatus::Partial)
    } else {
        (source_fidelity, HistoryReplayStatus::Ready)
    };
    if parsed.malformed {
        replay_entries.push(HistoryReplayEntry::Notice {
            code: "history_replay_malformed".to_string(),
        });
    }
    if parsed.truncated {
        replay_entries.push(HistoryReplayEntry::Notice {
            code: "history_replay_truncated".to_string(),
        });
    }
    Some(HistoryReplayResult::new(
        text_parts.join("\n"),
        fidelity,
        status,
        replay_entries,
    ))
}

fn read_bounded(path: &Path, max_bytes: usize) -> io::Result<(Vec<u8>, bool)> {
    let max_bytes = max_bytes.max(1);
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;
    let truncated = bytes.len() > max_bytes;
    if truncated {
        bytes.truncate(max_bytes);
    }
    Ok((bytes, truncated))
}

fn parse_transcript(bytes: &[u8], truncated: bool) -> ParsedTranscript {
    let mut parsed = ParsedTranscript {
        entries: Vec::new(),
        malformed: false,
        truncated,
    };
    for (source_index, line) in bytes.split(|byte| *byte == b'\n').enumerate() {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let value = match serde_json::from_slice::<Value>(line) {
            Ok(value) => value,
            Err(_) => {
                parsed.malformed = true;
                continue;
            }
        };
        if !value.is_object() {
            parsed.malformed = true;
            continue;
        }
        let step_index = value
            .get("step_index")
            .and_then(Value::as_u64)
            .unwrap_or(source_index as u64);
        append_record(&mut parsed, &value, step_index, source_index);
    }
    parsed
}

fn append_record(
    parsed: &mut ParsedTranscript,
    value: &Value,
    step_index: u64,
    source_index: usize,
) {
    let record_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_uppercase();
    let source = value
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_uppercase();
    let content = value.get("content").map(value_text).unwrap_or_default();
    let mut sub_index = 0;

    if record_type == "USER_INPUT" {
        if !extract_user_text(&content).is_empty() {
            push_entry(
                parsed,
                step_index,
                source_index,
                &mut sub_index,
                HistoryReplayEntry::UserMessage,
            );
        }
        return;
    }

    if (record_type == "PLANNER_RESPONSE" && source == "MODEL") || record_type == "AGENT_RESPONSE" {
        let text = content.trim();
        if !text.is_empty() {
            push_entry(
                parsed,
                step_index,
                source_index,
                &mut sub_index,
                HistoryReplayEntry::AssistantText {
                    text: text.to_string(),
                },
            );
        }
        if let Some(tool_calls) = value.get("tool_calls").and_then(Value::as_array) {
            for call in tool_calls {
                // Transcript result records carry the step index rather than
                // the planner call id. Keep one deterministic item namespace so
                // the replay lifecycle folds onto one logical tool item.
                let item_id = format!("step-{step_index}-{}", sub_index);
                let name = call
                    .get("name")
                    .and_then(Value::as_str)
                    .or_else(|| {
                        call.get("function")
                            .and_then(|function| function.get("name"))
                            .and_then(Value::as_str)
                    })
                    .or_else(|| {
                        call.get("args")
                            .and_then(|args| args.get("toolSummary"))
                            .and_then(Value::as_str)
                    })
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .unwrap_or("tool")
                    .to_string();
                push_entry(
                    parsed,
                    step_index,
                    source_index,
                    &mut sub_index,
                    HistoryReplayEntry::ToolStart { item_id, name },
                );
            }
        }
        return;
    }

    let result_types = [
        "RUN_COMMAND",
        "CODE_ACTION",
        "VIEW_FILE",
        "GREP_SEARCH",
        "LIST_DIRECTORY",
        "ERROR_MESSAGE",
        "GENERIC",
    ];
    if matches!(source.as_str(), "MODEL" | "SYSTEM") && result_types.contains(&record_type.as_str())
    {
        let success = !value
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(|status| status.eq_ignore_ascii_case("ERROR"));
        push_entry(
            parsed,
            step_index,
            source_index,
            &mut sub_index,
            HistoryReplayEntry::ToolResult {
                item_id: format!("step-{step_index}-0"),
                success,
            },
        );
    }
}

fn push_entry(
    parsed: &mut ParsedTranscript,
    step_index: u64,
    source_index: usize,
    sub_index: &mut usize,
    entry: HistoryReplayEntry,
) {
    parsed.entries.push(OrderedEntry {
        step_index,
        source_index,
        sub_index: *sub_index,
        entry,
    });
    *sub_index = sub_index.saturating_add(1);
}

fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(values) => values
            .iter()
            .map(value_text)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(object) => object
            .get("text")
            .or_else(|| object.get("content"))
            .map(value_text)
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn extract_user_text(content: &str) -> String {
    let content = content.trim();
    if let Some(start) = content.find("<USER_REQUEST>") {
        let start = start + "<USER_REQUEST>".len();
        if let Some(end) = content[start..].find("</USER_REQUEST>") {
            return content[start..start + end].trim().to_string();
        }
    }
    content.to_string()
}

fn has_visible_entries(result: &HistoryReplayResult) -> bool {
    result
        .entries
        .iter()
        .any(|entry| !matches!(entry, HistoryReplayEntry::Notice { .. }))
}

fn degraded_fallback(mut result: HistoryReplayResult) -> HistoryReplayResult {
    result.fidelity = HistoryReplayFidelity::Simplified;
    result.status = HistoryReplayStatus::Partial;
    result.entries.push(HistoryReplayEntry::Notice {
        code: "history_replay_full_fallback".to_string(),
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transcript(root: &Path, file_name: &str, lines: &[Value]) {
        let logs = root
            .join("brain")
            .join("REAL_CONVERSATION_ID")
            .join(".system_generated")
            .join("logs");
        fs::create_dir_all(&logs).unwrap();
        fs::write(
            logs.join(file_name),
            lines
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .unwrap();
    }

    fn assistant(step_index: u64, text: &str) -> Value {
        serde_json::json!({
            "step_index": step_index,
            "source": "MODEL",
            "type": "PLANNER_RESPONSE",
            "content": text,
        })
    }

    #[test]
    fn full_transcript_is_preferred_and_sorted_by_step() {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-history-reader-{}",
            uuid::Uuid::new_v4()
        ));
        transcript(
            &root,
            FULL_TRANSCRIPT,
            &[
                assistant(2, "second"),
                assistant(1, "first"),
                serde_json::json!({
                    "step_index": 3,
                    "source": "MODEL",
                    "type": "PLANNER_RESPONSE",
                    "tool_calls": [{
                        "function": {"name": "read_fixture"},
                        "args": {"secret": "RAW_TOOL_PAYLOAD"}
                    }]
                }),
                serde_json::json!({
                    "step_index": 3,
                    "source": "SYSTEM",
                    "type": "RUN_COMMAND",
                    "status": "DONE",
                    "content": "RAW_TOOL_PAYLOAD"
                }),
            ],
        );
        transcript(&root, SIMPLIFIED_TRANSCRIPT, &[assistant(1, "short")]);

        let result = AntigravityProviderHistoryReader::from_root(&root)
            .replay_sync("REAL_CONVERSATION_ID", 1024);
        assert_eq!(result.text, "first\nsecond");
        assert_eq!(result.fidelity, HistoryReplayFidelity::Full);
        assert_eq!(result.status, HistoryReplayStatus::Ready);
        assert!(result.entries.iter().any(|entry| matches!(
            entry,
            HistoryReplayEntry::ToolStart { name, .. } if name == "read_fixture"
        )));
        assert!(result
            .entries
            .iter()
            .any(|entry| matches!(entry, HistoryReplayEntry::ToolResult { success: true, .. })));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn malformed_full_falls_back_to_simplified_with_partial_status() {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-history-reader-{}",
            uuid::Uuid::new_v4()
        ));
        let logs = root.join("brain/REAL_CONVERSATION_ID/.system_generated/logs");
        fs::create_dir_all(&logs).unwrap();
        fs::write(logs.join(FULL_TRANSCRIPT), b"{malformed").unwrap();
        transcript(&root, SIMPLIFIED_TRANSCRIPT, &[assistant(1, "short")]);
        let result = AntigravityProviderHistoryReader::from_root(&root)
            .replay_sync("REAL_CONVERSATION_ID", 1024);
        assert_eq!(result.text, "short");
        assert_eq!(result.fidelity, HistoryReplayFidelity::Simplified);
        assert_eq!(result.status, HistoryReplayStatus::Partial);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn malformed_partial_and_missing_sources_are_explicit() {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-history-reader-{}",
            uuid::Uuid::new_v4()
        ));
        transcript(
            &root,
            FULL_TRANSCRIPT,
            &[assistant(1, "kept"), serde_json::json!("not an object")],
        );
        let partial = AntigravityProviderHistoryReader::from_root(&root)
            .replay_sync("REAL_CONVERSATION_ID", 1024);
        assert_eq!(partial.text, "kept");
        assert_eq!(partial.fidelity, HistoryReplayFidelity::Partial);
        assert_eq!(partial.status, HistoryReplayStatus::Partial);

        let missing = AntigravityProviderHistoryReader::from_root(root.join("missing"))
            .replay_sync("REAL_CONVERSATION_ID", 1024);
        assert_eq!(missing.fidelity, HistoryReplayFidelity::Unavailable);
        assert_eq!(missing.status, HistoryReplayStatus::Unavailable);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bounded_transcript_keeps_valid_prefix_and_reports_partial() {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-history-reader-{}",
            uuid::Uuid::new_v4()
        ));
        let first = assistant(1, "kept").to_string();
        let logs = root.join("brain/REAL_CONVERSATION_ID/.system_generated/logs");
        fs::create_dir_all(&logs).unwrap();
        fs::write(
            logs.join(FULL_TRANSCRIPT),
            format!("{first}\n{{\"step_index\":2,\"content\":\"cut").as_bytes(),
        )
        .unwrap();

        let result = AntigravityProviderHistoryReader::from_root(&root)
            .replay_sync("REAL_CONVERSATION_ID", first.len() + 2);
        assert_eq!(result.text, "kept");
        assert_eq!(result.fidelity, HistoryReplayFidelity::Partial);
        assert_eq!(result.status, HistoryReplayStatus::Partial);
        assert!(result.entries.iter().any(|entry| matches!(
            entry,
            HistoryReplayEntry::Notice { code } if code == "history_replay_truncated"
        )));
        let _ = fs::remove_dir_all(root);
    }
}
