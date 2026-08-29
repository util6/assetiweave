use std::collections::BTreeSet;
use std::sync::Arc;

use super::{
    lifecycle::AgentLifecycleService,
    types::{AgentInstallStartRequest, Distribution},
    AgentRuntimeManager,
};

/// Materialize only Agent IDs selected by the canonical settings model.
/// Package-manager based entries are intentionally left for explicit Market
/// installation, so an upgrade never performs network installation.
pub(crate) async fn migrate_legacy_assignments(
    pool: sqlx::SqlitePool,
    manager: Arc<AgentRuntimeManager>,
    scope_key: &str,
) -> Result<Vec<String>, String> {
    let settings = crate::backend::app_settings::load_or_import_app_settings_sqlx(&pool)
        .await
        .map_err(|error| error.to_string())?;
    let catalog = crate::backend::agent_market::CatalogCache::best_available()
        .map_err(|error| error.to_string())?;
    let catalog_version = catalog.catalog().catalog_version.clone();

    let mut agent_ids = BTreeSet::new();
    if let Some(assignments) = settings
        .get("agentAssignments")
        .and_then(serde_json::Value::as_object)
    {
        for agent_id in assignments.values().filter_map(|assignment| {
            assignment
                .get("agentId")
                .and_then(serde_json::Value::as_str)
        }) {
            let trimmed = agent_id.trim();
            if !trimmed.is_empty() {
                agent_ids.insert(trimmed.to_string());
            }
        }
    }
    if agent_ids.is_empty() {
        return Ok(Vec::new());
    }
    let scope_id = migration_scope_id(scope_key);
    let processed_ids = settings
        .get("agentMarketMigration")
        .and_then(serde_json::Value::as_object)
        .and_then(|value| value.get("scopes"))
        .and_then(serde_json::Value::as_object)
        .and_then(|scopes| scopes.get(&scope_id))
        .filter(|value| {
            value
                .get("catalogVersion")
                .and_then(serde_json::Value::as_str)
                == Some(catalog_version.as_str())
        })
        .and_then(|value| value.get("processedAgentIds"))
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    agent_ids.retain(|agent_id| !processed_ids.contains(agent_id.as_str()));
    if agent_ids.is_empty() {
        return Ok(Vec::new());
    }
    let processed_ids = processed_ids
        .into_iter()
        .map(str::to_string)
        .chain(agent_ids.iter().cloned())
        .collect::<BTreeSet<_>>();

    let mut notices = Vec::new();
    let runtime_root = super::default_runtime_root().map_err(|error| error.to_string())?;
    let settings_pool = pool.clone();
    let lifecycle = AgentLifecycleService::new(pool, manager, runtime_root)
        .map_err(|error| error.to_string())?;
    for agent_id in agent_ids {
        if lifecycle.repository.get(&agent_id).await?.is_some() {
            continue;
        }
        let Some(item) = catalog.item(&agent_id) else {
            notices.push(format!("{agent_id}: catalog item unavailable"));
            continue;
        };
        let Some(distribution) = item
            .distributions
            .iter()
            .find(|distribution| matches!(distribution, Distribution::System { .. }))
        else {
            notices.push(format!(
                "{agent_id}: managed installation requires confirmation"
            ));
            continue;
        };
        let request = AgentInstallStartRequest {
            agent_id: item.id.clone(),
            action: "install".to_string(),
            catalog_version: catalog_version.clone(),
            agent_version: item.version.clone(),
            distribution_id: distribution.id().to_string(),
            preview_token: catalog.preview_token(item, distribution.id(), "install"),
        };
        match lifecycle
            .install_with_cancellation_and_progress(request, None, None)
            .await
        {
            Ok(outcome) => {
                if !outcome.installation.execution_ready() {
                    notices.push(format!("{agent_id}: installed with degraded health"));
                }
            }
            Err(error) => notices.push(format!("{agent_id}: {}", error.code)),
        }
    }

    let mut updated_settings = settings;
    let marker = serde_json::json!({
        "catalogVersion": catalog_version,
        "completedAt": chrono::Utc::now().to_rfc3339(),
        "processedAgentIds": processed_ids,
        "notices": notices,
    });
    let root = updated_settings
        .as_object_mut()
        .ok_or_else(|| "application settings must be an object".to_string())?;
    let migration = root
        .entry("agentMarketMigration".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !migration.is_object() {
        *migration = serde_json::json!({});
    }
    migration
        .as_object_mut()
        .expect("migration marker object")
        .entry("scopes".to_string())
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .expect("migration scopes object")
        .insert(scope_id.clone(), marker);
    crate::backend::store::save_app_settings_sqlx(
        &settings_pool,
        crate::backend::app_settings::SETTINGS_SCHEMA_VERSION,
        &updated_settings,
    )
    .await
    .map_err(|error| error.to_string())?;
    Ok(
        updated_settings["agentMarketMigration"]["scopes"][migration_scope_id(scope_key)]
            ["notices"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_string)
            .collect(),
    )
}

fn migration_scope_id(scope_key: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    digest.update(scope_key.as_bytes());
    digest
        .finalize()
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
