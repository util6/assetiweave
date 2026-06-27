use crate::backend::dto::AppResult;
use crate::backend::models::{
    AuthMode, Principal, PrincipalKind, RequestContext, Tenant, TenantKind, TenantMembership,
    TenantRole, TenantStatus,
};
use chrono::Utc;
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

use super::{codec::decode_enum, sql};

pub(crate) const LOCAL_PRINCIPAL_ID: &str = "local";
pub(crate) const DEFAULT_TENANT_ID: &str = "default";

pub(crate) async fn ensure_local_identity_sqlx(pool: &SqlitePool) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query(sql::UPSERT_LOCAL_PRINCIPAL)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(sql::UPSERT_DEFAULT_TENANT)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(sql::UPSERT_DEFAULT_TENANT_MEMBERSHIP)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(sql::UPSERT_LOCAL_TENANT_STATE)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) async fn load_principal_sqlx(
    pool: &SqlitePool,
    principal_id: &str,
) -> AppResult<Option<Principal>> {
    sqlx::query(sql::LOAD_PRINCIPAL)
        .bind(principal_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(map_principal_row)
        .transpose()
}

pub(crate) async fn load_tenant_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Option<Tenant>> {
    sqlx::query(sql::LOAD_TENANT)
        .bind(tenant_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(map_tenant_row)
        .transpose()
}

pub(crate) async fn load_tenant_membership_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    principal_id: &str,
) -> AppResult<Option<TenantMembership>> {
    sqlx::query(sql::LOAD_TENANT_MEMBERSHIP)
        .bind(tenant_id)
        .bind(principal_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(map_tenant_membership_row)
        .transpose()
}

pub(crate) async fn list_tenants_for_principal_sqlx(
    pool: &SqlitePool,
    principal_id: &str,
) -> AppResult<Vec<Tenant>> {
    let rows = sqlx::query(sql::LIST_TENANTS_BY_PRINCIPAL)
        .bind(principal_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    rows.iter().map(map_tenant_row).collect()
}

pub(crate) async fn load_active_tenant_id_sqlx(
    pool: &SqlitePool,
    principal_id: &str,
) -> AppResult<Option<String>> {
    sqlx::query_scalar::<_, String>(sql::LOAD_ACTIVE_TENANT_ID)
        .bind(principal_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())
}

pub(crate) async fn set_active_tenant_sqlx(
    pool: &SqlitePool,
    principal_id: &str,
    tenant_id: &str,
) -> AppResult<Tenant> {
    let tenant = load_tenant_sqlx(pool, tenant_id)
        .await?
        .ok_or_else(|| format!("tenant not found: {tenant_id}"))?;
    load_tenant_membership_sqlx(pool, tenant_id, principal_id)
        .await?
        .ok_or_else(|| format!("principal {principal_id} is not a member of tenant {tenant_id}"))?;
    let now = Utc::now().to_rfc3339();
    let result = sqlx::query(sql::UPDATE_ACTIVE_TENANT)
        .bind(principal_id)
        .bind(tenant_id)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    if result.rows_affected() == 0 {
        return Err(format!("principal not found: {principal_id}"));
    }
    Ok(tenant)
}

pub(crate) async fn create_local_tenant_sqlx(
    pool: &SqlitePool,
    principal_id: &str,
    name: &str,
    slug: Option<&str>,
) -> AppResult<Tenant> {
    let name = clean_tenant_name(name)?;
    let slug = normalize_tenant_slug(slug.unwrap_or(&name))?;
    if load_tenant_sqlx(pool, &slug).await?.is_some() {
        return Err(format!("tenant already exists: {slug}"));
    }
    load_principal_sqlx(pool, principal_id)
        .await?
        .ok_or_else(|| format!("principal not found: {principal_id}"))?;

    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query(sql::INSERT_TENANT)
        .bind(&slug)
        .bind(&slug)
        .bind(&name)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(sql::INSERT_TENANT_MEMBERSHIP)
        .bind(&slug)
        .bind(principal_id)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;

    load_tenant_sqlx(pool, &slug)
        .await?
        .ok_or_else(|| format!("tenant not found after create: {slug}"))
}

pub(crate) async fn load_local_request_context_sqlx(
    pool: &SqlitePool,
) -> AppResult<RequestContext> {
    ensure_local_identity_sqlx(pool).await?;
    let principal = load_principal_sqlx(pool, LOCAL_PRINCIPAL_ID)
        .await?
        .ok_or_else(|| "local principal not found".to_string())?;
    let active_tenant_id = load_active_tenant_id_sqlx(pool, &principal.id)
        .await?
        .unwrap_or_else(|| DEFAULT_TENANT_ID.to_string());
    let tenant = load_tenant_sqlx(pool, &active_tenant_id)
        .await?
        .ok_or_else(|| format!("active tenant not found: {active_tenant_id}"))?;
    let membership = load_tenant_membership_sqlx(pool, &tenant.id, &principal.id)
        .await?
        .ok_or_else(|| {
            format!(
                "principal {} is not a member of tenant {}",
                principal.id, tenant.id
            )
        })?;
    Ok(RequestContext {
        principal,
        tenant,
        membership,
        auth_mode: AuthMode::Local,
    })
}

fn clean_tenant_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("tenant name is required".to_string());
    }
    Ok(name.to_string())
}

fn normalize_tenant_slug(value: &str) -> AppResult<String> {
    let mut slug = value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        return Err("tenant slug is required".to_string());
    }
    Ok(slug)
}

fn map_principal_row(row: &SqliteRow) -> AppResult<Principal> {
    Ok(Principal {
        id: row.try_get(0).map_err(|error| error.to_string())?,
        kind: decode_enum::<PrincipalKind>(
            row.try_get::<String, _>(1)
                .map_err(|error| error.to_string())?,
        )?,
        display_name: row.try_get(2).map_err(|error| error.to_string())?,
        created_at: row.try_get(3).map_err(|error| error.to_string())?,
        updated_at: row.try_get(4).map_err(|error| error.to_string())?,
    })
}

fn map_tenant_row(row: &SqliteRow) -> AppResult<Tenant> {
    Ok(Tenant {
        id: row.try_get(0).map_err(|error| error.to_string())?,
        slug: row.try_get(1).map_err(|error| error.to_string())?,
        name: row.try_get(2).map_err(|error| error.to_string())?,
        kind: decode_enum::<TenantKind>(
            row.try_get::<String, _>(3)
                .map_err(|error| error.to_string())?,
        )?,
        status: decode_enum::<TenantStatus>(
            row.try_get::<String, _>(4)
                .map_err(|error| error.to_string())?,
        )?,
        created_at: row.try_get(5).map_err(|error| error.to_string())?,
        updated_at: row.try_get(6).map_err(|error| error.to_string())?,
    })
}

fn map_tenant_membership_row(row: &SqliteRow) -> AppResult<TenantMembership> {
    Ok(TenantMembership {
        tenant_id: row.try_get(0).map_err(|error| error.to_string())?,
        principal_id: row.try_get(1).map_err(|error| error.to_string())?,
        role: decode_enum::<TenantRole>(
            row.try_get::<String, _>(2)
                .map_err(|error| error.to_string())?,
        )?,
        created_at: row.try_get(3).map_err(|error| error.to_string())?,
        updated_at: row.try_get(4).map_err(|error| error.to_string())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::store::Database;
    use uuid::Uuid;

    #[test]
    fn tenant_repo_loads_local_request_context() {
        let db_path =
            std::env::temp_dir().join(format!("assetiweave-tenant-{}.sqlite", Uuid::new_v4()));
        let database = Database::open_initialized(&db_path).expect("open initialized database");

        let context = database
            .block_on(async { load_local_request_context_sqlx(database.pool()).await })
            .expect("load local request context");

        assert_eq!(context.principal.id, LOCAL_PRINCIPAL_ID);
        assert_eq!(context.principal.kind, PrincipalKind::Local);
        assert_eq!(context.tenant.id, DEFAULT_TENANT_ID);
        assert_eq!(context.membership.role, TenantRole::Owner);
        assert_eq!(context.auth_mode, AuthMode::Local);
        drop(database);
        cleanup_database(&db_path);
    }

    fn cleanup_database(db_path: &std::path::Path) {
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    }
}
