pub(super) fn slug_path_segment(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;
    for character in value.trim().chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            last_was_separator = false;
        } else if !last_was_separator && !slug.is_empty() {
            slug.push('-');
            last_was_separator = true;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "source".to_string()
    } else {
        slug
    }
}

pub(super) fn strip_json_fence(value: &str) -> &str {
    let trimmed = value.trim();
    trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed)
}
