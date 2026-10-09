use crate::backend::store::StoreResult;
use sqlx::SqlitePool;

pub async fn checkpoint_database_wal_sqlx(pool: &SqlitePool) -> StoreResult<()> {
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(pool)
        .await?;
    Ok(())
}
