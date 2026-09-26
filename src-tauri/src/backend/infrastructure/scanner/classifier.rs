use super::prelude::*;

pub(super) fn classify_asset(
    source: &Source,
    path: &Path,
    relative_path: &str,
    format: AssetFormat,
) -> AssetKind {
    let context = super::detector::DetectionCtx {
        relative_path,
        format,
    };
    if let Some((_, _, detection)) = super::detector::detect(&context) {
        return detection.kind;
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
        .unwrap_or("");
    crate::backend::domain::catalog::detect_format_from_extension(extension)
}

pub(super) fn extract_description(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    crate::backend::domain::catalog::extract_description_from_text(&text)
}
