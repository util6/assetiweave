use super::prelude::*;

pub(super) fn scan_mixed_assets(source: &Source) -> AppResult<Vec<Asset>> {
    let root = expand_path(&source.root_path)?;
    if !root.exists() {
        return Err(format!("source path does not exist: {}", root.display()));
    }
    if !root.is_dir() {
        return Err(format!(
            "source path is not a directory: {}",
            root.display()
        ));
    }

    let include_set = build_glob_set(&source.include_globs, &["**/*"])?;
    let exclude_set = build_glob_set(&source.exclude_globs, &[])?;
    let mut assets_by_id = HashMap::new();
    let mut seen_skill_dirs = HashSet::new();
    let now = Utc::now().to_rfc3339();

    for entry in WalkDir::new(&root).follow_links(false) {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let relative = match path.strip_prefix(&root) {
            Ok(relative) if !relative.as_os_str().is_empty() => relative,
            _ => continue,
        };
        let relative_string = normalize_relative_path(relative);

        if exclude_set.is_match(&relative_string) {
            continue;
        }

        if entry.file_type().is_file()
            && path.file_name().and_then(|name| name.to_str()) == Some("SKILL.md")
        {
            if !include_set.is_match(&relative_string) {
                continue;
            }
            let skill_dir = path.parent().unwrap_or(path);
            let skill_relative = skill_dir.strip_prefix(&root).unwrap_or(relative);
            let skill_relative_string = normalize_relative_path(skill_relative);
            if seen_skill_dirs.insert(skill_relative_string.clone()) {
                let asset = build_asset(
                    source,
                    &root,
                    skill_dir,
                    &skill_relative_string,
                    Some(path),
                    AssetKind::Skill,
                    AssetFormat::Directory,
                    &now,
                )?;
                assets_by_id.insert(asset.id.clone(), asset);
            }
            continue;
        }

        if !entry.file_type().is_file() || !include_set.is_match(&relative_string) {
            continue;
        }

        let format = detect_format(path);
        let kind = classify_asset(source, path, &relative_string, format);
        if matches!(format, AssetFormat::Unknown) && matches!(kind, AssetKind::Unclassified) {
            continue;
        }

        let asset = build_asset(
            source,
            &root,
            path,
            &relative_string,
            None,
            kind,
            format,
            &now,
        )?;
        assets_by_id.insert(asset.id.clone(), asset);
    }

    let mut assets: Vec<_> = assets_by_id.into_values().collect();
    assets.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(assets)
}
