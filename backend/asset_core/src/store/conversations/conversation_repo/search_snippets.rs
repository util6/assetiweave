use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn push_search_hit_if_matching(
    hits: &mut Vec<ConversationSearchHit>,
    needle: &str,
    allowed_types: &BTreeSet<ConversationSearchCardType>,
    session: &ConversationSessionListItem,
    question: &ConversationQuestion,
    question_index: i64,
    question_title: &str,
    turn_id: Option<String>,
    part_id: Option<String>,
    block_id: String,
    card_type: ConversationSearchCardType,
    text: &str,
    id_fragment: Option<&str>,
    related_ids: &[&str],
) {
    if !allowed_types.is_empty() && !allowed_types.contains(&card_type) {
        return;
    }
    let matched_by_id = id_fragment.is_some_and(|fragment| {
        related_ids
            .iter()
            .any(|value| crate::backend::domain::conversation_id_fragment(value) == fragment)
    });
    if !matched_by_id && !text.to_lowercase().contains(needle) {
        return;
    }

    hits.push(ConversationSearchHit {
        session: session.clone(),
        question_id: question.id.clone(),
        question_index,
        question_title: question_title.to_string(),
        turn_id,
        part_id,
        block_id,
        card_type,
        snippet: if matched_by_id {
            leading_search_snippet(text)
        } else {
            search_snippet(text, needle)
        },
        score: if matched_by_id {
            12_000
        } else {
            match_count(text, needle) * 100
        },
        incremental: None,
        highlight_segments: None,
    });
}

pub(super) fn search_highlight_segments(
    text: &str,
    needle: &str,
) -> Option<Vec<crate::backend::domain::conversations::ConversationSearchHighlightSegment>> {
    let start = text.find(needle).or_else(|| {
        (text.is_ascii() && needle.is_ascii())
            .then(|| text.to_ascii_lowercase().find(needle))
            .flatten()
    })?;
    let end = start + needle.len();
    if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return None;
    }
    let mut segments = Vec::new();
    if start > 0 {
        segments.push(
            crate::backend::domain::conversations::ConversationSearchHighlightSegment {
                text: text[..start].to_string(),
                matched: false,
            },
        );
    }
    segments.push(
        crate::backend::domain::conversations::ConversationSearchHighlightSegment {
            text: text[start..end].to_string(),
            matched: true,
        },
    );
    if end < text.len() {
        segments.push(
            crate::backend::domain::conversations::ConversationSearchHighlightSegment {
                text: text[end..].to_string(),
                matched: false,
            },
        );
    }
    Some(segments)
}

pub(super) fn search_snippet(text: &str, needle: &str) -> String {
    let normalized_text = text.to_lowercase();
    let match_start = normalized_text
        .find(needle)
        .map(|index| normalized_text[..index].chars().count())
        .unwrap_or(0);
    let chars = text.chars().collect::<Vec<_>>();
    let start = match_start.saturating_sub(64);
    let end = (match_start + needle.chars().count() + 96).min(chars.len());
    let prefix = if start > 0 { "..." } else { "" };
    let suffix = if end < chars.len() { "..." } else { "" };
    compact_whitespace(&format!(
        "{prefix}{}{suffix}",
        chars[start..end].iter().collect::<String>()
    ))
}

pub(super) fn leading_search_snippet(text: &str) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let end = 104.min(chars.len());
    let suffix = if end < chars.len() { "..." } else { "" };
    compact_whitespace(&format!(
        "{}{suffix}",
        chars[..end].iter().collect::<String>()
    ))
}

pub(super) fn match_count(text: &str, needle: &str) -> usize {
    text.to_lowercase().matches(needle).count().max(1)
}
