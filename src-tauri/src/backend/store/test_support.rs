use crate::backend::store::{StoreError, StoreResult};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use uuid::Uuid;

static INITIALIZED_DB_PATHS: OnceLock<Mutex<BTreeSet<PathBuf>>> = OnceLock::new();

impl super::Database {
    pub(crate) async fn open_initialized_async(db_path: &Path) -> StoreResult<Self> {
        let pool = super::open_migrated_pool(db_path).await?;
        let initialized_paths = INITIALIZED_DB_PATHS.get_or_init(|| Mutex::new(BTreeSet::new()));
        let mut initialized_paths = initialized_paths.lock().map_err(StoreError::external)?;
        if !initialized_paths.contains(db_path) {
            crate::backend::application::system::test_support::init_test_database(&pool)
                .await
                .map_err(StoreError::external)?;
            initialized_paths.insert(db_path.to_path_buf());
        }
        Ok(Self::from_pool(pool))
    }
}

pub(crate) async fn create_test_pool() -> sqlx::SqlitePool {
    let path = std::env::temp_dir().join(format!("assetiweave-test-{}.sqlite", Uuid::new_v4()));
    let pool = crate::backend::store::open_migrated_pool(&path)
        .await
        .expect("open test pool");
    crate::backend::application::system::test_support::seed_defaults_sqlx(&pool)
        .await
        .expect("seed defaults");
    pool
}
