use super::ConversationGroupingOrigin;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationGroupSeed {
    pub turn_ids: Vec<String>,
    pub origin: ConversationGroupingOrigin,
}

pub fn conversation_id_fragment(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    find_conversation_hash(&normalized)
        .or_else(|| find_legacy_conversation_hash(&normalized))
        .unwrap_or(normalized.as_str())
        .chars()
        .take(8)
        .collect()
}

pub fn conversation_id_search_term(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() == 8 && normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Some(normalized);
    }
    if is_conversation_domain_id(&normalized)
        && (find_conversation_hash(&normalized).is_some()
            || find_legacy_conversation_hash(&normalized).is_some())
    {
        Some(normalized)
    } else {
        None
    }
}

fn is_conversation_domain_id(value: &str) -> bool {
    [
        "conversation-session-",
        "conversation-question-",
        "conversation-turn-",
        "conversation-part-",
        "web-record-session-",
        "web-record-question-",
        "web-record-turn-",
        "web-record-part-",
    ]
    .iter()
    .any(|prefix| value.starts_with(prefix))
}

fn find_conversation_hash(value: &str) -> Option<&str> {
    find_hex_run(value, 64)
}

fn find_legacy_conversation_hash(value: &str) -> Option<&str> {
    find_hex_run_at_least(value, 12)
}

fn find_hex_run(value: &str, expected_length: usize) -> Option<&str> {
    let bytes = value.as_bytes();
    if bytes.len() < expected_length {
        return None;
    }
    let mut start = 0;
    while start < bytes.len() {
        if !bytes[start].is_ascii_hexdigit() {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < bytes.len() && bytes[end].is_ascii_hexdigit() {
            end += 1;
        }
        if end - start == expected_length {
            return value.get(start..end);
        }
        start = end;
    }
    None
}

fn find_hex_run_at_least(value: &str, minimum_length: usize) -> Option<&str> {
    let bytes = value.as_bytes();
    if bytes.len() < minimum_length {
        return None;
    }
    let mut start = 0;
    while start < bytes.len() {
        if !bytes[start].is_ascii_hexdigit() {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < bytes.len() && bytes[end].is_ascii_hexdigit() {
            end += 1;
        }
        if end - start >= minimum_length {
            return value.get(start..end);
        }
        start = end;
    }
    None
}

pub fn should_auto_merge_acknowledgement(user_text: &str) -> bool {
    let normalized = user_text.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.contains('\n')
        || normalized.contains("```")
        || normalized.contains('?')
        || normalized.contains('？')
    {
        return false;
    }

    matches!(
        normalized.as_str(),
        "ok" | "okay"
            | "yes"
            | "y"
            | "no"
            | "n"
            | "continue"
            | "go ahead"
            | "proceed"
            | "确认"
            | "可以"
            | "好的"
            | "好"
            | "继续"
            | "继续吧"
            | "是"
            | "否"
            | "不用"
            | "不需要"
    )
}

fn should_auto_merge_interruption_recovery(user_text: &str) -> bool {
    let normalized = user_text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.contains('?')
        || normalized.contains('？')
        || normalized.contains("```")
    {
        return false;
    }

    [
        "continue where we left off",
        "pick up where we left off",
        "resume where we left off",
        "continue the previous question",
        "继续上一个问题",
        "接着刚才",
        "从刚才继续",
        "恢复刚才",
        "回到刚才",
    ]
    .iter()
    .any(|prefix| normalized == *prefix || normalized.starts_with(&format!("{prefix} ")))
}

fn should_auto_merge_micro_follow_up(user_text: &str) -> bool {
    let normalized = user_text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.chars().count() > 48
        || normalized.contains('?')
        || normalized.contains('？')
        || normalized.contains("```")
    {
        return false;
    }

    [
        "change it to ",
        "change that to ",
        "make it ",
        "switch it to ",
        "rename it to ",
        "改成",
        "改为",
        "换成",
        "加上",
        "删掉",
        "去掉",
        "把它",
        "调整",
    ]
    .iter()
    .any(|prefix| normalized.starts_with(prefix))
}

pub fn group_turn_ids_by_question<I>(turns: I) -> Vec<ConversationGroupSeed>
where
    I: IntoIterator<Item = (String, String)>,
{
    let mut groups: Vec<ConversationGroupSeed> = Vec::new();
    for (turn_id, user_text) in turns {
        if should_auto_merge_acknowledgement(&user_text)
            || should_auto_merge_interruption_recovery(&user_text)
            || should_auto_merge_micro_follow_up(&user_text)
        {
            if let Some(previous) = groups.last_mut() {
                previous.turn_ids.push(turn_id);
                if previous.origin == ConversationGroupingOrigin::Imported {
                    previous.origin = ConversationGroupingOrigin::AutoMerged;
                }
                continue;
            }
        }
        groups.push(ConversationGroupSeed {
            turn_ids: vec![turn_id],
            origin: ConversationGroupingOrigin::Imported,
        });
    }
    groups
}
