#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConversationAdapterEntryPathError {
    Rooted,
    ParentTraversal,
}

/// Validates the platform-independent path constraints for adapter manifest entries.
/// Resolving the path against an adapter directory remains an Infrastructure concern.
pub(crate) fn validate_conversation_adapter_entry_path(
    raw: &str,
) -> Result<(), ConversationAdapterEntryPathError> {
    let trimmed = raw.trim();
    if is_rooted_path(trimmed) {
        return Err(ConversationAdapterEntryPathError::Rooted);
    }
    if trimmed
        .split(['/', '\\'])
        .any(|component| component == "..")
    {
        return Err(ConversationAdapterEntryPathError::ParentTraversal);
    }
    Ok(())
}

fn is_rooted_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || path.starts_with('\\')
        || (bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic())
}
