use crate::backend::application::prelude::AppResult;
use crate::backend::domain::{
    ConversationAdapter, ConversationAdapterPackage, ConversationAdapterPackageVersion,
};
use sqlx::SqlitePool;

pub(crate) async fn list_adapters(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<ConversationAdapter>> {
    crate::backend::store::list_conversation_adapters_sqlx(pool, tenant_id)
        .await?
        .into_iter()
        .map(normalize_adapter)
        .collect()
}

pub(crate) async fn load_adapter(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: &str,
) -> AppResult<Option<ConversationAdapter>> {
    crate::backend::store::load_conversation_adapter_sqlx(pool, tenant_id, adapter_id)
        .await?
        .map(normalize_adapter)
        .transpose()
}

pub(crate) async fn save_adapter(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter: &ConversationAdapter,
) -> AppResult<()> {
    let adapter = normalize_adapter(adapter.clone())?;
    Ok(crate::backend::store::upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter).await?)
}

pub(crate) async fn list_packages(pool: &SqlitePool) -> AppResult<Vec<ConversationAdapterPackage>> {
    crate::backend::store::list_conversation_adapter_packages_sqlx(pool)
        .await?
        .into_iter()
        .map(normalize_package)
        .collect()
}

pub(crate) async fn load_package(
    pool: &SqlitePool,
    package_id: &str,
) -> AppResult<Option<ConversationAdapterPackage>> {
    crate::backend::store::load_conversation_adapter_package_sqlx(pool, package_id)
        .await?
        .map(normalize_package)
        .transpose()
}

pub(crate) async fn load_package_by_adapter(
    pool: &SqlitePool,
    adapter_id: &str,
) -> AppResult<Option<ConversationAdapterPackage>> {
    crate::backend::store::load_conversation_adapter_package_by_adapter_sqlx(pool, adapter_id)
        .await?
        .map(normalize_package)
        .transpose()
}

pub(crate) async fn save_package(
    pool: &SqlitePool,
    package: &ConversationAdapterPackage,
) -> AppResult<()> {
    let package = normalize_package(package.clone())?;
    Ok(crate::backend::store::upsert_conversation_adapter_package_sqlx(pool, &package).await?)
}

pub(crate) async fn list_package_versions(
    pool: &SqlitePool,
    package_id: &str,
) -> AppResult<Vec<ConversationAdapterPackageVersion>> {
    crate::backend::store::list_conversation_adapter_package_versions_sqlx(pool, package_id)
        .await?
        .into_iter()
        .map(normalize_version)
        .collect()
}

pub(crate) async fn activate_package(
    pool: &SqlitePool,
    adapter: &ConversationAdapter,
    package: &ConversationAdapterPackage,
    version: &ConversationAdapterPackageVersion,
) -> AppResult<()> {
    let adapter = normalize_adapter(adapter.clone())?;
    let package = normalize_package(package.clone())?;
    let version = normalize_version(version.clone())?;
    Ok(
        crate::backend::store::activate_conversation_adapter_package_sqlx(
            pool, &adapter, &package, &version,
        )
        .await?,
    )
}

pub(crate) async fn activate_workspace(
    pool: &SqlitePool,
    adapter: &ConversationAdapter,
    package: &ConversationAdapterPackage,
) -> AppResult<()> {
    let adapter = normalize_adapter(adapter.clone())?;
    let package = normalize_package(package.clone())?;
    Ok(
        crate::backend::store::activate_conversation_adapter_workspace_sqlx(
            pool, &adapter, &package,
        )
        .await?,
    )
}

pub(crate) async fn deactivate_package(
    pool: &SqlitePool,
    package_id: &str,
    adapter_id: &str,
) -> AppResult<ConversationAdapterPackage> {
    normalize_package(
        crate::backend::store::deactivate_conversation_adapter_package_sqlx(
            pool, package_id, adapter_id,
        )
        .await?,
    )
}

fn normalize_adapter(mut adapter: ConversationAdapter) -> AppResult<ConversationAdapter> {
    crate::backend::infrastructure::path_utils::normalize_conversation_adapter_paths(&mut adapter)?;
    Ok(adapter)
}

fn normalize_package(
    mut package: ConversationAdapterPackage,
) -> AppResult<ConversationAdapterPackage> {
    crate::backend::infrastructure::path_utils::normalize_conversation_adapter_package_paths(
        &mut package,
    )?;
    Ok(package)
}

fn normalize_version(
    mut version: ConversationAdapterPackageVersion,
) -> AppResult<ConversationAdapterPackageVersion> {
    crate::backend::infrastructure::path_utils::normalize_conversation_adapter_version_paths(
        &mut version,
    )?;
    Ok(version)
}

#[cfg(test)]
#[path = "conversation_storage_tests.rs"]
mod tests;
