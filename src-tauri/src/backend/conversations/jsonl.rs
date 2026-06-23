use super::prelude::*;

#[derive(Clone, Copy)]
pub(super) enum ParserFlavor {
    Codex,
    ClaudeCode,
}

pub(super) fn parse_jsonl_conversation(
    text: &str,
    flavor: ParserFlavor,
) -> AppResult<Vec<NormalizedConversationTurn>> {
    let mut turns = Vec::new();
    let mut current: Option<NormalizedConversationTurn> = None;

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let timestamp = string_field_any(&value, &["timestamp", "created_at", "updated_at"]);
        if matches!(flavor, ParserFlavor::ClaudeCode)
            && value.get("isSidechain").and_then(Value::as_bool) == Some(true)
        {
            if let Some(turn) = current.as_mut() {
                let text = extract_text(event_payload(&value));
                if !text.trim().is_empty() {
                    let part = NormalizedConversationPart {
                        role: ConversationPartRole::Assistant,
                        kind: ConversationPartKind::Subagent,
                        text: Some(text),
                        language: None,
                        command: None,
                        cwd: None,
                        status: None,
                        exit_code: None,
                        metadata_json: None,
                    };
                    let raw_metadata = compact_json(&value);
                    turn.parts
                        .push(with_declared_content_card(part, Some(&raw_metadata)));
                }
            }
            continue;
        }
        let payload = event_payload(&value);
        let role = role_of(payload).or_else(|| role_of(&value));
        let record_type = string_field_any(payload, &["type"])
            .or_else(|| string_field_any(&value, &["type"]))
            .unwrap_or_default();

        if let Some(user_text) =
            real_user_text_for_event(&value, payload, flavor, role.as_deref(), &record_type)
        {
            if let Some(turn) = current.take() {
                turns.push(turn);
            }
            current = Some(NormalizedConversationTurn {
                external_id: string_field_any(payload, &["id", "uuid"])
                    .or_else(|| string_field_any(&value, &["id", "uuid"]))
                    .unwrap_or_else(|| format!("turn-{}", turns.len())),
                turn_index: turns.len() as i64,
                user_text,
                title: None,
                started_at: timestamp,
                ended_at: None,
                parts: Vec::new(),
            });
            continue;
        }

        let Some(turn) = current.as_mut() else {
            continue;
        };
        if matches!(flavor, ParserFlavor::ClaudeCode) && is_user_tool_result_message(payload) {
            if let Some(part) = tool_part_from_event(payload) {
                turn.parts.push(part);
            }
            turn.ended_at = timestamp;
            continue;
        }
        if role.as_deref() == Some("assistant") {
            let text = extract_text(payload);
            if !text.trim().is_empty() {
                turn.parts.extend(
                    split_markdown_text_parts(ConversationPartRole::Assistant, &text)
                        .into_iter()
                        .map(|part| with_declared_content_card(part, None)),
                );
            }
            turn.ended_at = timestamp;
            continue;
        }
        if is_tool_record(&record_type, payload) {
            if let Some(part) = tool_part_from_event(payload) {
                turn.parts.push(part);
            }
            turn.ended_at = timestamp;
        }
    }
    if let Some(turn) = current {
        turns.push(turn);
    }
    Ok(turns)
}

pub(super) fn tool_part_from_event(value: &Value) -> Option<NormalizedConversationPart> {
    let command = command_from_value(value);
    let text = tool_text_from_event(value);
    let record_type = string_field_any(value, &["type"]).unwrap_or_default();
    let tool_name = tool_name_from_value(value);
    let fallback_text = tool_name.as_ref().map(|name| {
        let event_type = if record_type.is_empty() {
            "tool".to_string()
        } else {
            record_type.clone()
        };
        format!("{event_type}: {name}")
    });
    let text = if text.trim().is_empty() {
        fallback_text.unwrap_or_default()
    } else {
        text
    };
    if command.is_none() && text.trim().is_empty() {
        return None;
    }
    let kind = if is_file_change_tool(&record_type, tool_name.as_deref(), value) {
        ConversationPartKind::FileChange
    } else if command.is_some() || record_type.contains("shell") {
        ConversationPartKind::Command
    } else {
        ConversationPartKind::Tool
    };
    let part = NormalizedConversationPart {
        role: ConversationPartRole::Tool,
        kind,
        text: (!text.trim().is_empty()).then_some(text),
        language: None,
        command,
        cwd: cwd_from_value(value),
        status: status_from_value(value),
        exit_code: exit_code_from_value(value),
        metadata_json: None,
    };
    let raw_metadata = compact_json(value);
    Some(with_declared_content_card(part, Some(&raw_metadata)))
}

pub(super) fn is_tool_record(record_type: &str, payload: &Value) -> bool {
    record_type.contains("tool")
        || record_type.contains("function")
        || record_type.contains("exec")
        || record_type.contains("shell")
        || record_type == "patch"
        || payload.get("tool_use_id").is_some()
        || payload.get("toolUseID").is_some()
        || payload.get("call_id").is_some()
        || payload.get("callID").is_some()
        || payload.get("tool_name").is_some()
        || payload.get("toolName").is_some()
}

pub(super) fn is_file_change_tool(
    record_type: &str,
    tool_name: Option<&str>,
    value: &Value,
) -> bool {
    record_type == "patch"
        || tool_name.is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name.contains("apply_patch") || name.contains("patch") || name.contains("edit")
        })
        || value
            .get("arguments")
            .and_then(parse_json_string_value)
            .as_ref()
            .is_some_and(value_contains_file_change_key)
        || value
            .get("arguments")
            .is_some_and(value_contains_file_change_key)
}

pub(super) fn value_contains_file_change_key(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().any(value_contains_file_change_key),
        Value::Object(object) => {
            object.contains_key("patch")
                || object.contains_key("diff")
                || object.contains_key("file_change")
                || object.contains_key("fileChange")
                || object.values().any(value_contains_file_change_key)
        }
        _ => false,
    }
}

pub(super) fn event_payload(value: &Value) -> &Value {
    value
        .get("item")
        .or_else(|| value.get("message"))
        .or_else(|| value.get("msg"))
        .or_else(|| value.get("payload"))
        .unwrap_or(value)
}

pub(super) fn role_of(value: &Value) -> Option<String> {
    string_field_any(value, &["role"]).or_else(|| {
        value
            .get("message")
            .and_then(|message| string_field_any(message, &["role"]))
    })
}

pub(super) fn real_user_text_for_event(
    value: &Value,
    payload: &Value,
    flavor: ParserFlavor,
    role: Option<&str>,
    record_type: &str,
) -> Option<String> {
    if is_synthetic_user_event(value) || is_synthetic_user_event(payload) {
        return None;
    }
    if matches!(flavor, ParserFlavor::ClaudeCode) && is_user_tool_result_message(payload) {
        return None;
    }

    let is_user_boundary = match flavor {
        ParserFlavor::Codex => {
            role == Some("user") && is_message_like_payload(payload, record_type)
        }
        ParserFlavor::ClaudeCode => {
            (record_type == "user" && value.get("content").is_some())
                || (role == Some("user") && is_message_like_payload(payload, record_type))
        }
    };
    if !is_user_boundary {
        return None;
    }

    let text = extract_user_message_text(payload);
    clean_real_user_text(&text)
}

pub(super) fn is_message_like_payload(payload: &Value, record_type: &str) -> bool {
    record_type == "message" || payload.get("content").is_some() || payload.get("text").is_some()
}

pub(super) fn is_synthetic_user_event(value: &Value) -> bool {
    if is_ignored_content_value(value) {
        return true;
    }
    let event_type = string_field_any(value, &["type"]).unwrap_or_default();
    matches!(
        event_type.as_str(),
        "attachment"
            | "auth_status"
            | "compaction"
            | "compaction_summary"
            | "context_compaction"
            | "custom_tool_call"
            | "custom_tool_call_output"
            | "event_msg"
            | "function_call"
            | "function_call_output"
            | "grouped_tool_use"
            | "hook_result"
            | "image_generation_call"
            | "local_shell_call"
            | "mcp_tool_call"
            | "mcp_tool_call_output"
            | "progress"
            | "rate_limit_event"
            | "reasoning"
            | "result"
            | "system"
            | "tombstone"
            | "tool_result"
            | "tool_search_call"
            | "tool_search_output"
            | "tool_use"
            | "tool_use_summary"
            | "turn_context"
            | "web_search_call"
    ) || value.get("tool_use_id").is_some()
        || value.get("toolUseID").is_some()
        || value.get("call_id").is_some()
        || value.get("callID").is_some()
        || value.get("tool_name").is_some()
        || value.get("toolName").is_some()
}

pub(super) fn extract_user_message_text(value: &Value) -> String {
    let mut texts = Vec::new();
    if let Some(content) = value.get("content") {
        collect_user_content_text(content, &mut texts);
    } else if let Some(text) = value.get("text").and_then(Value::as_str) {
        texts.push(text.to_string());
    }
    texts.join("\n\n").trim().to_string()
}

pub(super) fn collect_user_content_text(value: &Value, texts: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            if !text.trim().is_empty() {
                texts.push(text.clone());
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_user_content_text(item, texts);
            }
        }
        Value::Object(object) => {
            let item_type = object.get("type").and_then(Value::as_str).unwrap_or("");
            if object_flag_true(object, &["synthetic", "ignored", "isSynthetic", "isMeta"]) {
                return;
            }
            if matches!(
                item_type,
                "attachment"
                    | "file"
                    | "hook_result"
                    | "image"
                    | "input_image"
                    | "reasoning"
                    | "thinking"
                    | "tool_result"
                    | "tool_use"
            ) {
                return;
            }
            if matches!(item_type, "" | "text" | "input_text" | "user" | "message") {
                if let Some(text) = object.get("text").and_then(Value::as_str) {
                    if !text.trim().is_empty() {
                        texts.push(text.to_string());
                    }
                    return;
                }
            }
            if let Some(input_text) = object.get("input_text").and_then(Value::as_str) {
                if !input_text.trim().is_empty() {
                    texts.push(input_text.to_string());
                }
                return;
            }
            if let Some(content) = object.get("content") {
                collect_user_content_text(content, texts);
            }
        }
        _ => {}
    }
}

pub(super) fn is_user_tool_result_message(value: &Value) -> bool {
    value
        .get("content")
        .is_some_and(|content| value_contains_type(content, "tool_result"))
}

pub(super) fn value_contains_type(value: &Value, expected_type: &str) -> bool {
    match value {
        Value::Array(items) => items
            .iter()
            .any(|item| value_contains_type(item, expected_type)),
        Value::Object(object) => {
            object.get("type").and_then(Value::as_str) == Some(expected_type)
                || object
                    .get("content")
                    .is_some_and(|content| value_contains_type(content, expected_type))
        }
        _ => false,
    }
}

pub(super) fn clean_real_user_text(text: &str) -> Option<String> {
    clean_real_user_fragment(text)
}

pub(super) fn clean_real_user_fragment(fragment: &str) -> Option<String> {
    let mut remaining = fragment.trim();
    loop {
        if remaining.starts_with("# AGENTS.md instructions for ") {
            let end = remaining.find("</INSTRUCTIONS>")?;
            remaining = remaining[end + "</INSTRUCTIONS>".len()..].trim_start();
            continue;
        }
        if remaining.starts_with("<environment_context") {
            let end = remaining.find("</environment_context>")?;
            remaining = remaining[end + "</environment_context>".len()..].trim_start();
            continue;
        }
        if remaining.starts_with("<codex_internal_context") {
            let end = remaining.find("</codex_internal_context>")?;
            remaining = remaining[end + "</codex_internal_context>".len()..].trim_start();
            continue;
        }
        if let Some(next) = strip_wrapped_prefix(remaining, "permissions instructions") {
            remaining = next;
            continue;
        }
        if let Some(next) = strip_wrapped_prefix(remaining, "app-context") {
            remaining = next;
            continue;
        }
        if let Some(next) = strip_wrapped_prefix(remaining, "personality_spec") {
            remaining = next;
            continue;
        }
        if let Some(next) = strip_wrapped_prefix(remaining, "skills_instructions") {
            remaining = next;
            continue;
        }
        if let Some(next) = strip_wrapped_prefix(remaining, "plugins_instructions") {
            remaining = next;
            continue;
        }
        break;
    }
    (!remaining.trim().is_empty()).then(|| remaining.trim().to_string())
}

pub(super) fn strip_wrapped_prefix<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    if !text.starts_with(&format!("<{tag}")) {
        return None;
    }
    let closing = format!("</{tag}>");
    let end = text.find(&closing)?;
    Some(text[end + closing.len()..].trim_start())
}

pub(super) fn opencode_part_text(kind: &str, value: &Value) -> Option<String> {
    if is_ignored_content_value(value) {
        return None;
    }
    let text = match kind {
        "text" => string_field_any(value, &["text", "content"]),
        "tool" | "tool-call" | "tool-result" => {
            let text = tool_text_from_event(value);
            (!text.trim().is_empty()).then_some(text)
        }
        "patch" => {
            string_field_any(value, &["text", "patch", "diff", "summary", "path"]).or_else(|| {
                let text = tool_text_from_event(value);
                (!text.trim().is_empty()).then_some(text)
            })
        }
        "file" => string_field_any(value, &["path", "filename", "name", "text", "summary"]),
        "subtask" => string_field_any(value, &["description", "prompt", "command", "agent"]),
        "agent" => string_field_any(value, &["agent", "summary", "text", "description"]),
        _ => {
            let text = extract_text(value);
            (!text.trim().is_empty()).then_some(text)
        }
    };
    text.filter(|text| !text.trim().is_empty())
}

pub(super) fn tool_text_from_event(value: &Value) -> String {
    let mut texts = Vec::new();
    for key in [
        "output",
        "tool_output",
        "toolOutput",
        "result",
        "content",
        "summary",
        "message",
        "error",
    ] {
        if let Some(child) = value.get(key) {
            collect_tool_text(child, &mut texts);
        }
    }
    if let Some(state) = value.get("state") {
        for key in ["title", "output", "error", "message"] {
            if let Some(child) = state.get(key) {
                collect_tool_text(child, &mut texts);
            }
        }
    }
    if texts.is_empty() {
        if let Some(arguments) = value.get("arguments") {
            if let Some(parsed) = parse_json_string_value(arguments) {
                collect_tool_text(&parsed, &mut texts);
            } else {
                collect_tool_text(arguments, &mut texts);
            }
        }
    }
    texts.join("\n").trim().to_string()
}

pub(super) fn collect_tool_text(value: &Value, texts: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            if !text.trim().is_empty() {
                texts.push(text.clone());
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_tool_text(item, texts);
            }
        }
        Value::Object(object) => {
            if object_flag_true(
                object,
                &[
                    "ignored",
                    "synthetic",
                    "isSynthetic",
                    "isMeta",
                    "isVisibleInTranscriptOnly",
                ],
            ) {
                return;
            }
            let item_type = object.get("type").and_then(Value::as_str).unwrap_or("");
            if matches!(
                item_type,
                "thinking" | "reasoning" | "step-start" | "step-finish" | "snapshot" | "retry"
            ) {
                return;
            }
            for key in [
                "text", "content", "output", "result", "summary", "stdout", "stderr", "preview",
                "message", "title", "patch", "diff",
            ] {
                if let Some(child) = object.get(key) {
                    collect_tool_text(child, texts);
                }
            }
        }
        _ => {}
    }
}

pub(super) fn command_from_value(value: &Value) -> Option<String> {
    command_from_value_inner(value, 0)
}

pub(super) fn command_from_value_inner(value: &Value, depth: usize) -> Option<String> {
    if depth > 8 {
        return None;
    }
    let object = value.as_object()?;
    if let Some(command) = string_field_any(value, &["command", "cmd", "shell_command"]) {
        return Some(command).filter(|value| !value.trim().is_empty());
    }
    for key in [
        "action",
        "input",
        "tool_input",
        "toolInput",
        "state",
        "request",
        "params",
        "parameters",
    ] {
        if let Some(child) = object.get(key) {
            if let Some(command) = command_from_value_inner(child, depth + 1) {
                return Some(command);
            }
        }
    }
    for key in ["arguments", "args"] {
        if let Some(child) = object.get(key) {
            if let Some(command) = command_from_arguments(child, depth + 1) {
                return Some(command);
            }
        }
    }
    None
}

pub(super) fn command_from_arguments(value: &Value, depth: usize) -> Option<String> {
    match value {
        Value::Object(_) => command_from_value_inner(value, depth),
        Value::Array(items) => {
            let args = items.iter().filter_map(Value::as_str).collect::<Vec<_>>();
            (!args.is_empty())
                .then(|| args.join(" "))
                .filter(|value| !value.trim().is_empty())
        }
        Value::String(_) => parse_json_string_value(value)
            .as_ref()
            .and_then(|parsed| command_from_value_inner(parsed, depth + 1)),
        _ => None,
    }
}

pub(super) fn parse_json_string_value(value: &Value) -> Option<Value> {
    let text = value.as_str()?.trim();
    if !text.starts_with('{') && !text.starts_with('[') {
        return None;
    }
    serde_json::from_str::<Value>(text).ok()
}

pub(super) fn extract_text(value: &Value) -> String {
    let mut texts = Vec::new();
    collect_text(value, &mut texts);
    texts.join("\n").trim().to_string()
}

pub(super) fn collect_text(value: &Value, texts: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            if !text.trim().is_empty() {
                texts.push(text.clone());
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_text(item, texts);
            }
        }
        Value::Object(object) => {
            let item_type = object.get("type").and_then(Value::as_str).unwrap_or("");
            if matches!(
                item_type,
                "thinking"
                    | "reasoning"
                    | "step-start"
                    | "step-finish"
                    | "compaction"
                    | "retry"
                    | "snapshot"
            ) {
                return;
            }
            for key in ["text", "content", "output", "result", "summary"] {
                if let Some(child) = object.get(key) {
                    collect_text(child, texts);
                }
            }
        }
        _ => {}
    }
}

pub(super) fn tool_name_from_value(value: &Value) -> Option<String> {
    string_field_any(value, &["tool_name", "toolName", "tool", "name"])
}

pub(super) fn cwd_from_value(value: &Value) -> Option<String> {
    nested_string_field_any(
        value,
        &["cwd", "workdir", "working_directory", "workingDirectory"],
        0,
    )
}

pub(super) fn status_from_value(value: &Value) -> Option<String> {
    nested_string_field_any(value, &["status", "state"], 0)
}

pub(super) fn exit_code_from_value(value: &Value) -> Option<i32> {
    nested_i64_field_any(value, &["exit_code", "exitCode", "code"], 0).map(|value| value as i32)
}

pub(super) fn nested_string_field_any(
    value: &Value,
    names: &[&str],
    depth: usize,
) -> Option<String> {
    if depth > 6 {
        return None;
    }
    if let Some(value) = value_field_any_as_string(value, names) {
        return Some(value).filter(|value| !value.trim().is_empty());
    }
    let object = value.as_object()?;
    for key in ["arguments", "args"] {
        if let Some(child) = object.get(key) {
            if let Some(parsed) = parse_json_string_value(child) {
                if let Some(value) = nested_string_field_any(&parsed, names, depth + 1) {
                    return Some(value);
                }
            } else if let Some(value) = nested_string_field_any(child, names, depth + 1) {
                return Some(value);
            }
        }
    }
    for key in ["state", "input", "tool_input", "toolInput", "action"] {
        if let Some(child) = object.get(key) {
            if let Some(value) = nested_string_field_any(child, names, depth + 1) {
                return Some(value);
            }
        }
    }
    None
}

pub(super) fn nested_i64_field_any(value: &Value, names: &[&str], depth: usize) -> Option<i64> {
    if depth > 6 {
        return None;
    }
    for name in names {
        if let Some(child) = value.get(*name) {
            if let Some(number) = child.as_i64() {
                return Some(number);
            }
            if let Some(text) = child.as_str().and_then(|text| text.parse::<i64>().ok()) {
                return Some(text);
            }
        }
    }
    let object = value.as_object()?;
    for key in ["arguments", "args"] {
        if let Some(child) = object.get(key) {
            if let Some(parsed) = parse_json_string_value(child) {
                if let Some(value) = nested_i64_field_any(&parsed, names, depth + 1) {
                    return Some(value);
                }
            } else if let Some(value) = nested_i64_field_any(child, names, depth + 1) {
                return Some(value);
            }
        }
    }
    for key in ["state", "input", "tool_input", "toolInput", "action"] {
        if let Some(child) = object.get(key) {
            if let Some(value) = nested_i64_field_any(child, names, depth + 1) {
                return Some(value);
            }
        }
    }
    None
}

pub(super) fn is_ignored_content_value(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object_flag_true(
            object,
            &[
                "ignored",
                "synthetic",
                "isSynthetic",
                "isMeta",
                "isVisibleInTranscriptOnly",
            ],
        )
    })
}

pub(super) fn object_flag_true(object: &serde_json::Map<String, Value>, names: &[&str]) -> bool {
    names
        .iter()
        .any(|name| object.get(*name).and_then(Value::as_bool) == Some(true))
}

pub(super) fn string_field_any(value: &Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_str).map(str::to_string))
}

pub(super) fn value_field_any_as_string(value: &Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(value_as_string))
}

pub(super) fn value_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

pub(super) fn infer_project_path_from_turns(
    turns: &[NormalizedConversationTurn],
) -> Option<String> {
    turns.iter().find_map(|turn| {
        turn.parts.iter().find_map(|part| {
            part.cwd
                .as_deref()
                .filter(|cwd| !cwd.trim().is_empty())
                .map(str::to_string)
        })
    })
}
