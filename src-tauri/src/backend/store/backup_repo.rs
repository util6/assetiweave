use crate::backend::runtime::{AppError, AppResult};
use sqlx::SqlitePool;

pub(crate) async fn checkpoint_database_wal_sqlx(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|error| AppError::External(error.to_string()))
}
