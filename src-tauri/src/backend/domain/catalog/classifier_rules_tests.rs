use super::*;

#[test]
fn test_detect_format_from_extension() {
    assert_eq!(detect_format_from_extension("md"), AssetFormat::Markdown);
    assert_eq!(detect_format_from_extension(".MDX"), AssetFormat::Markdown);
    assert_eq!(detect_format_from_extension("json"), AssetFormat::Json);
    assert_eq!(detect_format_from_extension("yaml"), AssetFormat::Yaml);
    assert_eq!(detect_format_from_extension(".yml"), AssetFormat::Yaml);
    assert_eq!(detect_format_from_extension("toml"), AssetFormat::Toml);
    assert_eq!(detect_format_from_extension("sh"), AssetFormat::Script);
    assert_eq!(detect_format_from_extension("py"), AssetFormat::Script);
    assert_eq!(detect_format_from_extension("sqlite"), AssetFormat::Sqlite);
    assert_eq!(detect_format_from_extension("db"), AssetFormat::Sqlite);
    assert_eq!(
        detect_format_from_extension("unknown_ext"),
        AssetFormat::Unknown
    );
    // Preserves legacy behavior: extensions with whitespace are not silently trimmed
    assert_eq!(detect_format_from_extension("md "), AssetFormat::Unknown);
    assert_eq!(detect_format_from_extension(" md"), AssetFormat::Unknown);
    assert_eq!(detect_format_from_extension(".md "), AssetFormat::Unknown);
    assert_eq!(detect_format_from_extension("   "), AssetFormat::Unknown);
    assert_eq!(detect_format_from_extension(""), AssetFormat::Unknown);
}

#[test]
fn test_extract_description_from_text_frontmatter() {
    let text =
        "---\nname: my-skill\ndescription: \"A test skill for testing\"\n---\n# Header\nContent";
    let desc = extract_description_from_text(text);
    assert_eq!(desc, Some("A test skill for testing".to_string()));
}

#[test]
fn test_extract_description_from_text_first_paragraph() {
    let text = "# Title\n\nThis is the description paragraph.\nSecond line.";
    let desc = extract_description_from_text(text);
    assert_eq!(desc, Some("This is the description paragraph.".to_string()));
}

#[test]
fn test_extract_description_from_text_empty() {
    assert_eq!(extract_description_from_text(""), None);
    assert_eq!(extract_description_from_text("# Only Title\n\n---"), None);
}
