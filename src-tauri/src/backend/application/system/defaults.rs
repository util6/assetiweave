//! Application use cases for preparing and persisting default tenant data.

use crate::backend::{
    application::system::default_data::{
        default_app_shortcuts, default_navigation_model, default_profiles_from_catalog,
        default_sources_for_tenant, is_default_app_profile_id,
    },
    application::AppResult,
    domain::TargetProfile,
    infrastructure::target_catalog::TargetCatalog,
    store::{
        self, count_rows_sqlx, ensure_default_app_shortcuts_sqlx, ensure_local_identity_sqlx,
        ensure_navigation_model_items_sqlx, load_profiles_sqlx, load_source_sqlx,
        load_sources_sqlx, seed_app_shortcuts_sqlx, seed_navigation_model_sqlx,
        upsert_profile_sqlx, upsert_source_sqlx, DEFAULT_TENANT_ID,
    },
};
use sqlx::SqlitePool;

/// Prepare the application-owned default data set and persist it before the
/// runtime publishes the loaded target catalog.
pub(crate) async fn seed_defaults_with_catalog(
    pool: &SqlitePool,
    catalog: &TargetCatalog,
) -> AppResult<()> {
    ensure_local_identity_sqlx(pool).await?;
    let tenant_id = DEFAULT_TENANT_ID;

    seed_tenant_defaults_with_catalog(pool, tenant_id, catalog).await?;
    normalize_all_tenant_paths_sqlx(pool).await?;

    Ok(())
}

pub(crate) async fn seed_tenant_defaults_with_catalog(
    pool: &SqlitePool,
    tenant_id: &str,
    catalog: &TargetCatalog,
) -> AppResult<()> {
    if count_rows_sqlx(pool, tenant_id, "sources").await? == 0 {
        for source in default_sources_for_tenant(tenant_id) {
            upsert_source_sqlx(pool, tenant_id, &source).await?;
        }
    }
    ensure_library_source_sqlx(pool, tenant_id).await?;
    ensure_system_skill_source_sqlx(pool, tenant_id).await?;
    normalize_existing_sources_sqlx(pool, tenant_id).await?;

    if count_rows_sqlx(pool, tenant_id, "profiles").await? == 0 {
        for profile in default_profiles_from_catalog(catalog) {
            let profile = normalize_profile_paths(profile)?;
            upsert_profile_sqlx(pool, tenant_id, &profile).await?;
        }
    } else {
        ensure_default_profiles_sqlx(pool, tenant_id, catalog).await?;
    }
    normalize_existing_profiles_sqlx(pool, tenant_id).await?;
    normalize_default_profiles_sqlx(pool, tenant_id, catalog).await?;

    let default_navigation_model = default_navigation_model();
    if count_rows_sqlx(pool, tenant_id, "navigation_state").await? == 0 {
        seed_navigation_model_sqlx(pool, tenant_id, &default_navigation_model).await?;
    } else {
        ensure_navigation_model_items_sqlx(pool, tenant_id, &default_navigation_model).await?;
    }

    if count_rows_sqlx(pool, tenant_id, "app_shortcut_items").await? == 0 {
        seed_app_shortcuts_sqlx(pool, tenant_id, &default_app_shortcuts()).await?;
    } else {
        ensure_default_app_shortcuts_sqlx(pool, tenant_id, &default_app_shortcuts()).await?;
    }

    Ok(())
}

async fn ensure_default_profiles_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    catalog: &TargetCatalog,
) -> AppResult<()> {
    let existing_profiles = load_profiles_sqlx(pool, tenant_id).await?;
    for profile in default_profiles_from_catalog(catalog) {
        if existing_profiles
            .iter()
            .any(|existing| existing.id == profile.id)
        {
            continue;
        }
        let profile = normalize_profile_paths(profile)?;
        upsert_profile_sqlx(pool, tenant_id, &profile).await?;
    }
    Ok(())
}

async fn ensure_library_source_sqlx(pool: &SqlitePool, tenant_id: &str) -> AppResult<()> {
    if load_source_sqlx(pool, tenant_id, "assetiweave-library-skills")
        .await?
        .is_some()
    {
        return Ok(());
    }
    if let Some(source) = default_sources_for_tenant(tenant_id)
        .into_iter()
        .find(|source| source.id == "assetiweave-library-skills")
    {
        upsert_source_sqlx(pool, tenant_id, &source).await?;
    }
    Ok(())
}

async fn ensure_system_skill_source_sqlx(pool: &SqlitePool, tenant_id: &str) -> AppResult<()> {
    let source =
        crate::backend::domain::system_skill_source("~/.assetiweave/skills/.system".to_string());
    Ok(upsert_source_sqlx(pool, tenant_id, &source).await?)
}

async fn normalize_existing_sources_sqlx(pool: &SqlitePool, tenant_id: &str) -> AppResult<()> {
    for source in load_sources_sqlx(pool, tenant_id).await? {
        upsert_source_sqlx(pool, tenant_id, &source).await?;
    }
    Ok(())
}

async fn normalize_existing_profiles_sqlx(pool: &SqlitePool, tenant_id: &str) -> AppResult<()> {
    for profile in load_profiles_sqlx(pool, tenant_id).await? {
        let profile = normalize_profile_paths(profile)?;
        upsert_profile_sqlx(pool, tenant_id, &profile).await?;
    }
    Ok(())
}

async fn normalize_all_tenant_paths_sqlx(pool: &SqlitePool) -> AppResult<()> {
    let tenant_ids = store::load_all_tenant_ids_sqlx(pool).await?;
    for tenant_id in tenant_ids {
        if tenant_id == DEFAULT_TENANT_ID {
            continue;
        }
        normalize_existing_sources_sqlx(pool, &tenant_id).await?;
        normalize_existing_profiles_sqlx(pool, &tenant_id).await?;
        crate::backend::application::system::conversation_adapters::normalize_conversation_paths(
            pool, &tenant_id,
        )
        .await?;
    }
    Ok(())
}

async fn normalize_default_profiles_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    catalog: &TargetCatalog,
) -> AppResult<()> {
    let defaults = default_profiles_from_catalog(catalog)
        .into_iter()
        .map(normalize_profile_paths)
        .collect::<AppResult<Vec<_>>>()?;
    let previous_defaults =
        crate::backend::application::system::default_data::builtin_profiles_for_migration()?
            .into_iter()
            .map(normalize_profile_paths)
            .collect::<AppResult<Vec<_>>>()?;
    for profile in load_profiles_sqlx(pool, tenant_id).await? {
        let mut profile = normalize_profile_paths(profile)?;
        let Some(default_profile) = defaults.iter().find(|candidate| candidate.id == profile.id)
        else {
            continue;
        };
        let matches_previous_catalog = previous_defaults
            .iter()
            .find(|candidate| candidate.id == profile.id)
            .is_some_and(|candidate| candidate.target_paths == profile.target_paths);
        if matches_previous_catalog
            || legacy_profile_target_paths(&profile.id).contains(&profile.target_paths)
        {
            profile.target_provider_id = default_profile.target_provider_id.clone();
            profile.target_paths = default_profile.target_paths.clone();
            upsert_profile_sqlx(pool, tenant_id, &profile).await?;
        }
    }
    Ok(())
}

fn normalize_profile_paths(mut profile: TargetProfile) -> AppResult<TargetProfile> {
    profile.target_paths = profile
        .target_paths
        .iter()
        .map(|path| crate::backend::infrastructure::path_utils::normalize_path_for_storage(path))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(profile)
}

fn legacy_profile_target_paths(profile_id: &str) -> Vec<Vec<String>> {
    if !is_default_app_profile_id(profile_id) && profile_id != "custom" {
        return Vec::new();
    }
    let legacy_path = match profile_id {
        "codex" => "~/.codex/assetiweave",
        "claude" => "~/.claude/assetiweave",
        "cursor" => "~/Library/Application Support/Cursor/assetiweave",
        "opencode" => "~/.opencode/assetiweave",
        "gemini" => "~/.gemini/assetiweave",
        "antigravity" => "~/.antigravity/assetiweave",
        "openclaw" => "~/.openclaw/assetiweave",
        "custom" => "~/assetiweave-target",
        _ => return Vec::new(),
    };
    let mut paths = vec![vec![legacy_path.to_string()]];
    if profile_id == "opencode" {
        paths.push(vec!["~/.opencode/skills".to_string()]);
    }
    paths
}

#[cfg(test)]
#[path = "defaults_tests.rs"]
mod tests;
