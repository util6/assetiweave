use super::prelude::*;

pub(super) fn scan_skill_assets(source: &Source) -> AppResult<Vec<Asset>> {
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

    let include_set = build_glob_set(&source.include_globs, &["**/SKILL.md"])?;
    let exclude_set = build_glob_set(&source.exclude_globs, &[])?;
    let mut assets_by_name = HashMap::new();
    let mut seen_skill_dirs = HashSet::new();
    let now = Utc::now().to_rfc3339();

    for entry in WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        let relative = match path.strip_prefix(&root) {
            Ok(relative) if !relative.as_os_str().is_empty() => relative,
            _ => continue,
        };
        let relative_string = normalize_relative_path(relative);

        if exclude_set.is_match(&relative_string) {
            continue;
        }

        if !entry.file_type().is_file()
            || path.file_name().and_then(|name| name.to_str()) != Some("SKILL.md")
            || !include_set.is_match(&relative_string)
        {
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
            insert_canonical_skill_asset(&mut assets_by_name, asset);
        }
    }

    let mut assets: Vec<_> = assets_by_name.into_values().collect();
    assets.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(assets)
}

fn insert_canonical_skill_asset(assets_by_name: &mut HashMap<String, Asset>, asset: Asset) {
    match assets_by_name.get(&asset.name) {
        Some(existing) if skill_asset_rank(existing) <= skill_asset_rank(&asset) => {}
        _ => {
            assets_by_name.insert(asset.name.clone(), asset);
        }
    }
}

fn skill_asset_rank(asset: &Asset) -> (usize, usize, String) {
    let parts: Vec<_> = asset.relative_path.split('/').collect();
    (
        locale_rank(&parts),
        parts.len(),
        asset.relative_path.clone(),
    )
}

fn locale_rank(parts: &[&str]) -> usize {
    for locale in ["zh-cn", "en-us"] {
        if parts.iter().any(|part| part.eq_ignore_ascii_case(locale)) {
            return if locale == "zh-cn" { 0 } else { 1 };
        }
    }
    2
}
