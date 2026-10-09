use super::AssetFormat;

/// Pure business rules for classifying asset format by file extension.
pub fn detect_format_from_extension(extension: &str) -> AssetFormat {
    match extension
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "md" | "mdx" => AssetFormat::Markdown,
        "json" => AssetFormat::Json,
        "yaml" | "yml" => AssetFormat::Yaml,
        "toml" => AssetFormat::Toml,
        "sh" | "bash" | "zsh" | "js" | "ts" | "py" => AssetFormat::Script,
        "sqlite" | "sqlite3" | "db" => AssetFormat::Sqlite,
        _ => AssetFormat::Unknown,
    }
}

/// Pure business rules for extracting asset description from plain text content.
pub fn extract_description_from_text(text: &str) -> Option<String> {
    for line in text.lines().map(str::trim) {
        if line.is_empty()
            || line == "---"
            || line.starts_with('#')
            || line.starts_with("name:")
            || line.starts_with("description:")
        {
            if let Some(description) = line.strip_prefix("description:") {
                let cleaned = description.trim().trim_matches('"').to_string();
                if !cleaned.is_empty() {
                    return Some(cleaned);
                }
            }
            continue;
        }
        return Some(line.chars().take(260).collect());
    }
    None
}

#[cfg(test)]
#[path = "classifier_rules_tests.rs"]
mod tests;
