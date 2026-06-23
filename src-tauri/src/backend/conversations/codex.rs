use super::prelude::*;

pub(super) fn read_claude_code_sessions(
    source: &ConversationSource,
) -> AppResult<Vec<NormalizedConversationSession>> {
    let root = crate::backend::path_utils::expand_path(&source.location)?;
    if !root.exists() {
        return Ok(Vec::new());
    }
    let files = if root.is_file() {
        vec![root]
    } else {
        collect_files_with_extension(&root, "jsonl", 1000)?
    };
    let mut sessions = Vec::new();
    for path in files {
        let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let turns = parse_jsonl_conversation(&text, ParserFlavor::ClaudeCode)?;
        if turns.is_empty() {
            continue;
        }
        let external_id = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("claude-session")
            .to_string();
        sessions.push(NormalizedConversationSession {
            external_id,
            title: path
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .map(|name| name.replace('-', "/")),
            project_path: infer_project_path_from_turns(&turns),
            started_at: turns.first().and_then(|turn| turn.started_at.clone()),
            updated_at: turns.last().and_then(|turn| turn.ended_at.clone()),
            source_locator: Some(path.to_string_lossy().to_string()),
            source_fingerprint: Some(hash_bytes(text.as_bytes())),
            turns,
        });
    }
    Ok(sessions)
}
