use super::prelude::*;

pub(crate) fn refresh_recorded_asset(
    source: &Source,
    asset: &Asset,
    timestamp: &str,
) -> AppResult<Option<Asset>> {
    let asset_path = Path::new(&asset.absolute_path);
    if !asset_path.exists() {
        return Ok(None);
    }

    let description_path = recorded_description_path(source, asset, asset_path);
    let content_path = if matches!(asset.format, AssetFormat::Directory) {
        asset_path
    } else {
        description_path.as_path()
    };
    let description = extract_description(&description_path);
    let content_hash = hash_path(content_path)?;
    let changed = asset.content_hash.as_deref() != Some(content_hash.as_str())
        || asset.description != description;

    let mut refreshed = asset.clone();
    refreshed.description = description;
    refreshed.content_hash = Some(content_hash);
    if changed {
        refreshed.updated_at = timestamp.to_string();
    }
    Ok(Some(refreshed))
}

pub(super) fn build_asset(
    source: &Source,
    root: &Path,
    asset_path: &Path,
    relative_path: &str,
    entry_file: Option<&Path>,
    kind: AssetKind,
    format: AssetFormat,
    timestamp: &str,
) -> AppResult<Asset> {
    let name = if matches!(format, AssetFormat::Directory) {
        asset_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(relative_path)
            .to_string()
    } else {
        asset_path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or(relative_path)
            .to_string()
    };
    let description_path = entry_file.unwrap_or(asset_path);
    let content_path = if matches!(format, AssetFormat::Directory) {
        asset_path
    } else {
        description_path
    };
    let description = extract_description(description_path);
    let content_hash = hash_path(content_path).ok();
    let id = stable_asset_id(&source.id, relative_path);
    let entry_file_relative = entry_file
        .and_then(|entry| entry.strip_prefix(root).ok())
        .map(normalize_relative_path);

    Ok(Asset {
        id,
        source_id: source.id.clone(),
        name,
        kind,
        format,
        relative_path: relative_path.to_string(),
        absolute_path: asset_path.to_string_lossy().to_string(),
        entry_file: entry_file_relative,
        description,
        content_hash,
        discovered_at: timestamp.to_string(),
        updated_at: timestamp.to_string(),
    })
}

fn recorded_description_path(
    source: &Source,
    asset: &Asset,
    asset_path: &Path,
) -> std::path::PathBuf {
    if let Some(entry_file) = asset.entry_file.as_deref() {
        if let Ok(root) = expand_path(&source.root_path) {
            let entry_path = root.join(entry_file);
            if entry_path.exists() {
                return entry_path;
            }
        }
    }

    if matches!(asset.format, AssetFormat::Directory) {
        let skill_entry = asset_path.join("SKILL.md");
        if skill_entry.exists() {
            return skill_entry;
        }
    }

    asset_path.to_path_buf()
}
