use std::collections::HashSet;

pub const OVERRIDE_MODEL_ID_PREFIX: &str = "__override__::";
pub const IGNORE_MODEL_ID_PREFIX: &str = "__ignore__::";

pub fn base_model_id_from_catalog_key(key: &str) -> &str {
    let trimmed = key.trim();
    trimmed
        .strip_prefix(OVERRIDE_MODEL_ID_PREFIX)
        .unwrap_or(trimmed)
}

pub(super) fn clean_base_id_from_pattern(
    pattern: &str,
    fallback_candidate: Option<&str>,
) -> String {
    let base = base_model_id_from_catalog_key(pattern).trim_matches('*');
    if !base.contains('*') && !base.is_empty() {
        return base.trim_start_matches('-').to_string();
    }
    if let Some(cand) = fallback_candidate {
        if !cand.is_empty() {
            return cand.to_string();
        }
    }
    base.to_string()
}

pub fn is_dirty_model_name(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return false;
    }
    if raw.contains('\n') || raw.contains('\r') {
        return true;
    }
    if trimmed.contains('"')
        || trimmed.contains('\'')
        || trimmed.contains('{')
        || trimmed.contains('}')
    {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("://") || lower.starts_with("www.") || lower.contains(" www.") {
        return true;
    }
    if trimmed.chars().count() > 96 {
        return true;
    }
    if trimmed.split_whitespace().count() > 8 {
        return true;
    }
    false
}

pub fn normalize_model_name(raw: &str) -> String {
    let trimmed = raw.trim().to_ascii_lowercase();
    if let Some(stripped) = trimmed.strip_prefix("models/") {
        stripped.trim().to_string()
    } else {
        trimmed
    }
}

fn is_thinking_or_effort_label(inner: &str) -> bool {
    let lower = inner.trim().to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "high"
            | "medium"
            | "med"
            | "low"
            | "minimal"
            | "min"
            | "thinking"
            | "think"
            | "reasoning"
            | "fast"
            | "default"
            | "auto"
            | "xhigh"
            | "extra high"
            | "extra-high"
            | "max"
            | "standard"
            | "none"
            | "off"
            | "on"
            | "adaptive"
    ) || lower.starts_with("effort:")
        || lower.starts_with("effort ")
        || lower.starts_with("thinking:")
        || lower.starts_with("thinking ")
        || lower.starts_with("reasoning:")
        || lower.starts_with("reasoning ")
}

fn strip_trailing_thinking_paren(name: &str) -> Option<String> {
    let trimmed = name.trim_end();
    if !trimmed.ends_with(')') {
        return None;
    }
    let open = trimmed.rfind('(')?;
    let inner = &trimmed[open + 1..trimmed.len() - 1];
    if !is_thinking_or_effort_label(inner) {
        return None;
    }
    let prefix = trimmed[..open].trim_end();
    if prefix.is_empty() {
        None
    } else {
        Some(prefix.to_string())
    }
}

fn strip_all_trailing_thinking_parens(name: &str) -> String {
    let mut cur = name.to_string();
    while let Some(stripped) = strip_trailing_thinking_paren(&cur) {
        cur = stripped;
    }
    cur
}

fn normalize_whitespace_and_hyphens(s: &str) -> String {
    let chars: Vec<char> = s.trim().chars().collect();
    let mut out = String::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch.is_whitespace() {
            while i + 1 < chars.len() && chars[i + 1].is_whitespace() {
                i += 1;
            }
            let next_ch = if i + 1 < chars.len() {
                Some(chars[i + 1])
            } else {
                None
            };
            let prev_ch = out.chars().last();
            let adjacent_sep = matches!(prev_ch, Some('-') | Some('/'))
                || matches!(next_ch, Some('-') | Some('/'));
            if !adjacent_sep && !out.is_empty() && next_ch.is_some() {
                out.push('-');
            }
        } else {
            out.push(ch);
        }
        i += 1;
    }
    out
}

pub fn candidate_model_names(raw: &str) -> Vec<String> {
    if is_dirty_model_name(raw) {
        return Vec::new();
    }

    let norm = normalize_model_name(raw);
    if norm.is_empty() {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    let mut push_unique = |val: String, list: &mut Vec<String>| {
        if !val.is_empty() && seen.insert(val.clone()) {
            list.push(val);
        }
    };

    push_unique(norm.clone(), &mut candidates);

    let paren_stripped = strip_all_trailing_thinking_parens(&norm);
    push_unique(paren_stripped.clone(), &mut candidates);

    let structural = normalize_whitespace_and_hyphens(&paren_stripped);
    push_unique(structural.clone(), &mut candidates);

    // Strip gateway prefixes & slash segments on initial structural forms
    let base_forms = candidates.clone();
    for form in base_forms {
        if let Some(stripped) = form.strip_prefix("openrouter/") {
            push_unique(stripped.to_string(), &mut candidates);
        }
        if let Some(last_slash) = form.rfind('/') {
            let tail = form[last_slash + 1..].to_string();
            push_unique(tail, &mut candidates);
            if let Some(first_slash) = form.find('/') {
                if first_slash != last_slash {
                    let sub = form[first_slash + 1..].to_string();
                    push_unique(sub, &mut candidates);
                }
            }
        }
    }

    // Multi-pass suffix & variant expansion (handles chained suffixes like -tail-free or -n (High))
    let mut idx = 0;
    const MAX_CANDIDATES: usize = 64;
    while idx < candidates.len() && candidates.len() < MAX_CANDIDATES {
        let name = candidates[idx].clone();
        idx += 1;

        if let Some(s) = strip_trailing_thinking_paren(&name) {
            let structural_s = normalize_whitespace_and_hyphens(&s);
            push_unique(s, &mut candidates);
            push_unique(structural_s, &mut candidates);
        }
        if let Some(s) = strip_free_suffix(&name) {
            push_unique(s, &mut candidates);
        }
        if let Some(s) = strip_date_suffix(&name) {
            push_unique(s, &mut candidates);
        }
        if let Some(s) = strip_alias_suffix(&name) {
            push_unique(s, &mut candidates);
        }
        if let Some(s) = generate_hyphen_variant(&name) {
            push_unique(s, &mut candidates);
        }
        if let Some(s) = strip_qwen_version(&name) {
            push_unique(s, &mut candidates);
        }
    }

    candidates
}

fn strip_date_suffix(name: &str) -> Option<String> {
    // 8-digit date like -20250219 or -20241022
    if name.len() >= 9 {
        let suffix = &name[name.len() - 9..];
        if suffix.starts_with('-') && suffix[1..].chars().all(|c| c.is_ascii_digit()) {
            return Some(name[..name.len() - 9].to_string());
        }
    }
    // YYYY-MM-DD like -2024-08-06 (11 chars)
    if name.len() >= 11 {
        let suffix = &name[name.len() - 11..];
        if suffix.starts_with('-') {
            let parts: Vec<&str> = suffix[1..].split('-').collect();
            if parts.len() == 3
                && parts[0].len() == 4
                && parts[0].chars().all(|c| c.is_ascii_digit())
                && parts[1].len() == 2
                && parts[1].chars().all(|c| c.is_ascii_digit())
                && parts[2].len() == 2
                && parts[2].chars().all(|c| c.is_ascii_digit())
            {
                return Some(name[..name.len() - 11].to_string());
            }
        }
    }
    // Shorter stamps like -001, -0125, -1106, -0613
    for len in [4, 5] {
        if name.len() >= len {
            let suffix = &name[name.len() - len..];
            if suffix.starts_with('-') && suffix[1..].chars().all(|c| c.is_ascii_digit()) {
                return Some(name[..name.len() - len].to_string());
            }
        }
    }
    None
}

fn strip_free_suffix(name: &str) -> Option<String> {
    for suffix in ["-free", ":free", "_free", "-chat-free", "-free-chat"] {
        if let Some(stripped) = name.strip_suffix(suffix) {
            if !stripped.is_empty() {
                return Some(stripped.to_string());
            }
        }
    }
    None
}

fn strip_alias_suffix(name: &str) -> Option<String> {
    for suffix in [
        "-latest",
        "-preview",
        "-preview-f",
        "-exp",
        "-experimental",
        "-chat",
        "-default",
        "-tail",
        "-n",
        "-f",
        "-turbo",
        "-instruct",
        ":batch",
        "-batch",
    ] {
        if let Some(stripped) = name.strip_suffix(suffix) {
            if !stripped.is_empty() {
                return Some(stripped.to_string());
            }
        }
    }
    if let Some(idx) = name.find("-preview") {
        let base = &name[..idx];
        if !base.is_empty() {
            return Some(base.to_string());
        }
    }
    None
}

pub fn generate_hyphen_variant(name: &str) -> Option<String> {
    // glm5 <-> glm-5
    if let Some(idx) = name.find("glm") {
        let after = &name[idx + 3..];
        if after.starts_with('-')
            && after[1..]
                .chars()
                .next()
                .map_or(false, |c| c.is_ascii_digit())
        {
            let mut s = String::with_capacity(name.len() - 1);
            s.push_str(&name[..idx + 3]);
            s.push_str(&after[1..]);
            return Some(s);
        } else if after.chars().next().map_or(false, |c| c.is_ascii_digit()) {
            let mut s = String::with_capacity(name.len() + 1);
            s.push_str(&name[..idx + 3]);
            s.push('-');
            s.push_str(after);
            return Some(s);
        }
    }
    // qwen3 <-> qwen-3
    if let Some(idx) = name.find("qwen") {
        let after = &name[idx + 4..];
        if after.starts_with('-')
            && after[1..]
                .chars()
                .next()
                .map_or(false, |c| c.is_ascii_digit())
        {
            let mut s = String::with_capacity(name.len() - 1);
            s.push_str(&name[..idx + 4]);
            s.push_str(&after[1..]);
            return Some(s);
        } else if after.chars().next().map_or(false, |c| c.is_ascii_digit()) {
            let mut s = String::with_capacity(name.len() + 1);
            s.push_str(&name[..idx + 4]);
            s.push('-');
            s.push_str(after);
            return Some(s);
        }
    }
    None
}

fn strip_qwen_version(name: &str) -> Option<String> {
    if let Some(stripped) = name.strip_prefix("qwen") {
        for sfx in ["-plus", "-max", "-turbo"] {
            if stripped.ends_with(sfx) {
                return Some(format!("qwen{}", sfx));
            }
        }
    }
    None
}

pub fn matches_pattern(pattern: &str, candidate: &str) -> bool {
    let pat = pattern.trim().to_ascii_lowercase();
    let cand = candidate.trim().to_ascii_lowercase();
    if pat == "*" || pat == cand {
        return true;
    }
    if !pat.contains('*') {
        return pat == cand;
    }

    let parts: Vec<&str> = pat.split('*').collect();
    let starts_with_wildcard = pat.starts_with('*');
    let ends_with_wildcard = pat.ends_with('*');

    let non_empty: Vec<&str> = parts.into_iter().filter(|s| !s.is_empty()).collect();
    if non_empty.is_empty() {
        return true;
    }

    let mut current_idx = 0;
    for (i, part) in non_empty.iter().enumerate() {
        if i == 0 && !starts_with_wildcard {
            if !cand.starts_with(part) {
                return false;
            }
            current_idx = part.len();
        } else if i == non_empty.len() - 1 && !ends_with_wildcard {
            if !cand[current_idx..].ends_with(part) {
                return false;
            }
        } else {
            match cand[current_idx..].find(part) {
                Some(found_idx) => {
                    current_idx += found_idx + part.len();
                }
                None => return false,
            }
        }
    }
    true
}

/// Calculates specificity score for pattern matching against candidate.
/// Exact match gets 10,000 + length. Wildcards get count of non-wildcard chars.
pub fn pattern_specificity(pattern: &str, candidate: &str) -> Option<usize> {
    if !matches_pattern(pattern, candidate) {
        return None;
    }
    let pat = pattern.trim().to_ascii_lowercase();
    let cand = candidate.trim().to_ascii_lowercase();
    if pat == cand {
        Some(10_000 + pat.len())
    } else {
        let non_wildcards = pat.chars().filter(|&c| c != '*').count();
        Some(non_wildcards)
    }
}
