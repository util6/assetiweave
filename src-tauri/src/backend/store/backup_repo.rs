use crate::backend::runtime::AppResult;
use sqlx::SqlitePool;

pub(crate) async fn checkpoint_database_wal_sqlx(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(pool)
        .await?;
    Ok(())
}
