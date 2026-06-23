use super::prelude::*;

pub(crate) fn read_source_sessions_with_adapter(
    adapter: Option<&ConversationAdapter>,
    source: &ConversationSource,
) -> AppResult<Vec<NormalizedConversationSession>> {
    let adapter =
        adapter.ok_or_else(|| format!("conversation adapter not found: {}", source.adapter_id))?;
    read_external_adapter_sessions(adapter, source)
}
