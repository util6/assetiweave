use crate::backend::domain::DeploymentState;
use crate::backend::store::StoreResult;
use sqlx::SqlitePool;

pub async fn upsert_deployment_state_sqlx(
    _pool: &SqlitePool,
    _tenant_id: &str,
    _state: &DeploymentState,
) -> StoreResult<()> {
    Ok(())
}

pub async fn is_managed_deployment_sqlx(
    _pool: &SqlitePool,
    _tenant_id: &str,
    _profile_id: &str,
    _asset_id: &str,
    _target_path: &str,
) -> StoreResult<bool> {
    Ok(false)
}

pub async fn load_managed_deployment_targets_by_profile_sqlx(
    _pool: &SqlitePool,
    _tenant_id: &str,
    _profile_id: &str,
) -> StoreResult<Vec<(String, String)>> {
    Ok(Vec::new())
}

#[cfg(test)]
#[path = "deployment_repo_tests.rs"]
mod tests;
