use crate::backend::dto::AppResult;
use sqlx::SqlitePool;

pub(crate) async fn vacuum_database_into_sqlx(
    pool: &SqlitePool,
    target_path: &str,
) -> AppResult<()> {
    sqlx::query("VACUUM main INTO ?")
        .bind(target_path)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub(crate) async fn checkpoint_database_wal_sqlx(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}
