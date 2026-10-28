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

/// 自动归并规则：确认语、打断恢复与微型追问都并入上一组。
pub fn should_merge_into_previous_question(user_text: &str) -> bool {
    should_auto_merge_acknowledgement(user_text)
        || should_auto_merge_interruption_recovery(user_text)
        || should_auto_merge_micro_follow_up(user_text)
}

/// 复合归并判定：子代理轮次、接续轮次或父级链接命中上一组轮次时直接并入上一组，不受文本长度或标点限制。
pub fn should_merge_turn_with_active_question(
    prompt: &str,
    turn_kind: Option<&str>,
    parent_turn_id: Option<&str>,
    active_turn_ids: &[&str],
) -> bool {
    if let Some(kind) = turn_kind {
        if matches!(
            kind,
            "subagent" | "agent" | "continuation" | "main_continuation"
        ) {
            if let Some(parent_id) = parent_turn_id {
                return active_turn_ids.iter().any(|id| *id == parent_id);
            }
            return true;
        }
    }
    if let Some(parent_id) = parent_turn_id {
        if active_turn_ids.iter().any(|id| *id == parent_id) {
            return true;
        }
    }
    should_merge_into_previous_question(prompt)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationTurnSeed {
    pub id: String,
    pub prompt: String,
    pub turn_kind: Option<String>,
    pub parent_turn_id: Option<String>,
}

impl From<(String, String)> for ConversationTurnSeed {
    fn from((id, prompt): (String, String)) -> Self {
        Self {
            id,
            prompt,
            turn_kind: None,
            parent_turn_id: None,
        }
    }
}

impl From<(&str, &str)> for ConversationTurnSeed {
    fn from((id, prompt): (&str, &str)) -> Self {
        Self {
            id: id.to_string(),
            prompt: prompt.to_string(),
            turn_kind: None,
            parent_turn_id: None,
        }
    }
}

impl From<(String, String, Option<String>, Option<String>)> for ConversationTurnSeed {
    fn from(
        (id, prompt, turn_kind, parent_turn_id): (String, String, Option<String>, Option<String>),
    ) -> Self {
        Self {
            id,
            prompt,
            turn_kind,
            parent_turn_id,
        }
    }
}

impl From<(&str, &str, Option<&str>, Option<&str>)> for ConversationTurnSeed {
    fn from(
        (id, prompt, turn_kind, parent_turn_id): (&str, &str, Option<&str>, Option<&str>),
    ) -> Self {
        Self {
            id: id.to_string(),
            prompt: prompt.to_string(),
            turn_kind: turn_kind.map(str::to_string),
            parent_turn_id: parent_turn_id.map(str::to_string),
        }
    }
}

impl From<&super::ConversationTurn> for ConversationTurnSeed {
    fn from(turn: &super::ConversationTurn) -> Self {
        Self {
            id: turn.id.clone(),
            prompt: turn.prompt.clone(),
            turn_kind: turn.turn_kind.clone(),
            parent_turn_id: turn.parent_turn_id.clone(),
        }
    }
}

impl From<super::ConversationTurn> for ConversationTurnSeed {
    fn from(turn: super::ConversationTurn) -> Self {
        Self {
            id: turn.id,
            prompt: turn.prompt,
            turn_kind: turn.turn_kind,
            parent_turn_id: turn.parent_turn_id,
        }
    }
}

pub fn group_turn_ids_by_question<I, T>(turns: I) -> Vec<ConversationGroupSeed>
where
    I: IntoIterator<Item = T>,
    T: Into<ConversationTurnSeed>,
{
    let mut groups: Vec<ConversationGroupSeed> = Vec::new();
    for turn in turns {
        let seed = turn.into();
        let should_merge = if let Some(previous) = groups.last() {
            let active_ids = previous
                .turn_ids
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            should_merge_turn_with_active_question(
                &seed.prompt,
                seed.turn_kind.as_deref(),
                seed.parent_turn_id.as_deref(),
                &active_ids,
            )
        } else {
            false
        };

        if should_merge {
            if let Some(previous) = groups.last_mut() {
                previous.turn_ids.push(seed.id);
                if previous.origin == ConversationGroupingOrigin::Imported {
                    previous.origin = ConversationGroupingOrigin::AutoMerged;
                }
                continue;
            }
        }
        groups.push(ConversationGroupSeed {
            turn_ids: vec![seed.id],
            origin: ConversationGroupingOrigin::Imported,
        });
    }
    groups
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicGroupedTurn<T> {
    pub question_id: String,
    pub origin: ConversationGroupingOrigin,
    pub turns: Vec<T>,
}

pub fn derive_question_id_from_turn_id(prefix: &str, turn_id: &str) -> String {
    let clean = if let Some(suffix) = turn_id.strip_prefix("conversation-turn-") {
        suffix
    } else if let Some(suffix) = turn_id.strip_prefix("web-record-turn-") {
        suffix
    } else {
        turn_id
    };
    format!("{prefix}-{clean}")
}

pub fn derive_turn_id_from_question_id(question_id: &str) -> Option<String> {
    if let Some(suffix) = question_id.strip_prefix("conversation-question-") {
        Some(format!("conversation-turn-{suffix}"))
    } else if let Some(suffix) = question_id.strip_prefix("web-record-question-") {
        Some(format!("web-record-turn-{suffix}"))
    } else {
        None
    }
}

/// 从轮次标识直接推导分组（问题）标识。
/// 若以 `web-record-turn-` 开头则派生为 `web-record-question-`；
/// 若以 `conversation-turn-` 开头则派生为 `conversation-question-`；
/// 其它自定义/测试标识原样返回。
pub fn derive_question_id_from_turn(turn_id: &str) -> String {
    if turn_id.starts_with("web-record-turn-") {
        derive_question_id_from_turn_id("web-record-question", turn_id)
    } else if turn_id.starts_with("conversation-turn-") {
        derive_question_id_from_turn_id("conversation-question", turn_id)
    } else {
        turn_id.to_string()
    }
}

fn extract_turn_kind<T: 'static>(turn: &T) -> Option<&str> {
    let any = turn as &dyn std::any::Any;
    if let Some(t) = any.downcast_ref::<super::ConversationTurn>() {
        return t.turn_kind.as_deref();
    }
    if let Some(t) = any.downcast_ref::<super::NormalizedConversationTurn>() {
        return t.turn_kind.as_deref();
    }
    if let Some(t) = any.downcast_ref::<ConversationTurnSeed>() {
        return t.turn_kind.as_deref();
    }
    None
}

fn extract_parent_turn_id<T: 'static>(turn: &T) -> Option<&str> {
    let any = turn as &dyn std::any::Any;
    if let Some(t) = any.downcast_ref::<super::ConversationTurn>() {
        return t.parent_turn_id.as_deref();
    }
    if let Some(t) = any.downcast_ref::<super::NormalizedConversationTurn>() {
        return t.parent_external_id.as_deref();
    }
    if let Some(t) = any.downcast_ref::<ConversationTurnSeed>() {
        return t.parent_turn_id.as_deref();
    }
    None
}

pub fn group_turns_by_dynamic_projection<T: 'static, F, I>(
    prefix: &str,
    turns: Vec<T>,
    prompt_extractor: F,
    id_extractor: I,
) -> Vec<DynamicGroupedTurn<T>>
where
    F: Fn(&T) -> &str,
    I: Fn(&T) -> &str,
{
    group_turns_by_dynamic_projection_with_meta(
        prefix,
        turns,
        prompt_extractor,
        id_extractor,
        |turn| extract_turn_kind(turn),
        |turn| extract_parent_turn_id(turn),
    )
}

pub fn group_turns_by_dynamic_projection_with_meta<T, F, I, K, P>(
    prefix: &str,
    turns: Vec<T>,
    prompt_extractor: F,
    id_extractor: I,
    kind_extractor: K,
    parent_id_extractor: P,
) -> Vec<DynamicGroupedTurn<T>>
where
    F: Fn(&T) -> &str,
    I: Fn(&T) -> &str,
    K: Fn(&T) -> Option<&str>,
    P: Fn(&T) -> Option<&str>,
{
    let mut groups: Vec<DynamicGroupedTurn<T>> = Vec::new();
    for turn in turns {
        let prompt = prompt_extractor(&turn);
        let turn_kind = kind_extractor(&turn);
        let parent_id = parent_id_extractor(&turn);

        let should_merge = if let Some(prev) = groups.last() {
            let active_ids = prev
                .turns
                .iter()
                .map(|t| id_extractor(t))
                .collect::<Vec<_>>();
            should_merge_turn_with_active_question(prompt, turn_kind, parent_id, &active_ids)
        } else {
            false
        };

        if should_merge {
            if let Some(prev) = groups.last_mut() {
                prev.origin = ConversationGroupingOrigin::AutoMerged;
                prev.turns.push(turn);
                continue;
            }
        }
        let turn_id = id_extractor(&turn);
        let question_id = derive_question_id_from_turn_id(prefix, turn_id);
        groups.push(DynamicGroupedTurn {
            question_id,
            origin: ConversationGroupingOrigin::Imported,
            turns: vec![turn],
        });
    }
    groups
}

#[cfg(test)]
#[path = "grouping_tests.rs"]
mod tests;
