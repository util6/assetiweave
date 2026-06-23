use super::prelude::*;

pub(super) fn classify_asset(
    source: &Source,
    path: &Path,
    relative_path: &str,
    format: AssetFormat,
) -> AssetKind {
    let lower = relative_path.to_lowercase();
    if lower.contains("prompt") {
        return AssetKind::Prompt;
    }
    if lower.contains("rule")
        || lower.contains(".cursorrules")
        || lower.contains("requirements")
        || lower.contains("design")
    {
        return AssetKind::Rule;
    }
    if lower.contains("memory") {
        return AssetKind::Memory;
    }
    if lower.contains("agent") {
        return AssetKind::Agent;
    }
    if lower.contains("workflow") {
        return AssetKind::Workflow;
    }
    if lower.contains("command") || lower.contains("slash") {
        return AssetKind::Command;
    }
    if matches!(
        format,
        AssetFormat::Json | AssetFormat::Yaml | AssetFormat::Toml
    ) && lower.contains("mcp")
    {
        return AssetKind::Mcp;
    }
    if let Some(default_kind) = source.default_kind {
        return default_kind;
    }
    if path.extension().and_then(|extension| extension.to_str()) == Some("md") {
        return AssetKind::Custom;
    }
    AssetKind::Unclassified
}

pub(super) fn detect_format(path: &Path) -> AssetFormat {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_lowercase();
    match extension.as_str() {
        "md" | "mdx" => AssetFormat::Markdown,
        "json" => AssetFormat::Json,
        "yaml" | "yml" => AssetFormat::Yaml,
        "toml" => AssetFormat::Toml,
        "sh" | "bash" | "zsh" | "js" | "ts" | "py" => AssetFormat::Script,
        "sqlite" | "sqlite3" | "db" => AssetFormat::Sqlite,
        _ => AssetFormat::Unknown,
    }
}

pub(super) fn extract_description(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
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
