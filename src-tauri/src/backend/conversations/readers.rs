use super::prelude::*;

pub(crate) fn read_source_sessions(
    source: &ConversationSource,
) -> AppResult<Vec<NormalizedConversationSession>> {
    match source.adapter_id.as_str() {
        "codex" => read_codex_sessions(source),
        "claude-code" => read_claude_code_sessions(source),
        "opencode" => read_opencode_sessions(source),
        _ => Err(format!(
            "external adapter sync is only available through conversation.adapter.try-run in this build: {}",
            source.adapter_id
        )),
    }
}

pub(crate) fn read_source_sessions_with_adapter(
    adapter: Option<&ConversationAdapter>,
    source: &ConversationSource,
) -> AppResult<Vec<NormalizedConversationSession>> {
    match source.adapter_id.as_str() {
        "codex" | "claude-code" | "opencode" => read_source_sessions(source),
        _ => {
            let adapter = adapter
                .ok_or_else(|| format!("conversation adapter not found: {}", source.adapter_id))?;
            read_external_adapter_sessions(adapter, source)
        }
    }
}
