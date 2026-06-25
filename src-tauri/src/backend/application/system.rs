use super::prelude::*;

impl AppService {
    pub(crate) fn open_for_engine() -> AppResult<Self> {
        Self::open_with_db_path(engine_db_path()?)
    }

    pub(crate) fn open_with_db_path(db_path: PathBuf) -> AppResult<Self> {
        let db = crate::backend::store::Database::open_initialized(&db_path)?;
        Ok(Self { db, db_path })
    }

    pub(crate) fn overview(&self) -> AppResult<AppOverview> {
        let pool = self.db.pool().clone();
        self.db.block_on(async move {
            Ok(AppOverview {
                source_count: crate::backend::store::count_rows_sqlx(&pool, "sources").await?,
                asset_count: crate::backend::store::count_rows_sqlx(&pool, "assets").await?,
                profile_count: crate::backend::store::count_rows_sqlx(&pool, "profiles").await?,
                last_scan_status: crate::backend::store::latest_scan_status_sqlx(&pool).await?,
            })
        })
    }

    pub(crate) fn logs_get_snapshot(
        &self,
        file_name: Option<String>,
        line_limit: Option<usize>,
    ) -> AppResult<crate::backend::logs::LogSnapshot> {
        crate::backend::logs::logs_get_snapshot(file_name, line_limit)
    }

    pub(crate) fn logs_open_log_directory(&self) -> AppResult<()> {
        crate::backend::logs::logs_open_log_directory()
    }

    pub(crate) fn logs_write_operation(
        &self,
        level: String,
        operation: String,
        message: String,
        fields: Option<BTreeMap<String, String>>,
    ) -> AppResult<()> {
        crate::backend::logs::logs_write_operation(level, operation, message, fields)
    }

    pub(crate) fn get_app_settings(
        &self,
    ) -> AppResult<crate::backend::app_settings::AppSettingsFile> {
        crate::backend::app_settings::get_app_settings()
    }

    pub(crate) fn save_app_settings(
        &self,
        settings: Value,
    ) -> AppResult<crate::backend::app_settings::AppSettingsFile> {
        crate::backend::app_settings::save_app_settings(settings)
    }

    pub(crate) fn run_doctor(&self) -> AppResult<Value> {
        let backup_root = capabilities::skill_backup_root_sqlx(&self.db)?;
        let runtime_statuses = self.list_conversation_adapter_runtime_statuses()?;
        let available_runtime_count = runtime_statuses
            .iter()
            .filter(|status| status.available)
            .count();
        let node_runtime_available = runtime_statuses.iter().any(|status| {
            status.kind == crate::backend::conversations::ConversationAdapterRuntimeKind::Node
                && status.available
        });
        let runtime_status = if node_runtime_available {
            "pass"
        } else {
            "warn"
        };
        let runtime_message = if node_runtime_available {
            format!(
                "{available_runtime_count}/{} runtimes available",
                runtime_statuses.len()
            )
        } else {
            format!(
                "node runtime missing; {available_runtime_count}/{} runtimes available",
                runtime_statuses.len()
            )
        };
        let pool = self.db.pool().clone();
        let source_count = self.db.block_on(async move {
            crate::backend::store::count_rows_sqlx(&pool, "sources").await
        })?;
        Ok(json!({
            "checks": [
                { "name": "database", "status": "pass", "message": self.db_path.to_string_lossy() },
                {
                    "name": "skill_backup_root",
                    "status": if backup_root.exists() { "pass" } else { "fail" },
                    "message": backup_root.to_string_lossy()
                },
                {
                    "name": "sources",
                    "status": "pass",
                    "message": format!("{source_count} sources")
                },
                {
                    "name": "conversation_adapter_runtimes",
                    "status": runtime_status,
                    "message": runtime_message,
                    "details": runtime_statuses
                }
            ]
        }))
    }
}

fn engine_db_path() -> AppResult<PathBuf> {
    if let Ok(path) = env::var("ASSETIWEAVE_DB_PATH") {
        if !path.trim().is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    crate::backend::path_utils::app_db_path()
}
