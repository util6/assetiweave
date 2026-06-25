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
        let (runtime_status, runtime_message) =
            conversation_runtime_doctor_summary(&runtime_statuses);
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

fn conversation_runtime_doctor_summary(
    runtime_statuses: &[crate::backend::conversations::ConversationAdapterRuntimeStatus],
) -> (&'static str, String) {
    let available_runtime_count = runtime_statuses
        .iter()
        .filter(|status| status.available)
        .count();
    let unavailable_required = runtime_statuses
        .iter()
        .filter(|status| status.required_version.is_some() && !status.available)
        .map(|status| {
            let requirement = status.required_version.as_deref().unwrap_or_default();
            format!("{:?} {requirement}", status.kind).to_ascii_lowercase()
        })
        .collect::<Vec<_>>();
    if unavailable_required.is_empty() {
        (
            "pass",
            format!(
                "{available_runtime_count}/{} runtimes available; all required adapter runtimes available",
                runtime_statuses.len()
            ),
        )
    } else {
        (
            "warn",
            format!(
                "missing required adapter runtimes: {}; {available_runtime_count}/{} runtimes available",
                unavailable_required.join(", "),
                runtime_statuses.len()
            ),
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::conversations::{
        ConversationAdapterRuntimeKind, ConversationAdapterRuntimeStatus,
    };

    #[test]
    fn runtime_doctor_ignores_unavailable_unrequired_runtimes() {
        let statuses = vec![
            runtime_status(ConversationAdapterRuntimeKind::Node, false, None),
            runtime_status(ConversationAdapterRuntimeKind::Python, true, Some(">=3.10")),
            runtime_status(ConversationAdapterRuntimeKind::Bash, true, None),
        ];

        let (status, message) = conversation_runtime_doctor_summary(&statuses);

        assert_eq!(status, "pass");
        assert!(message.contains("all required adapter runtimes available"));
        assert!(!message.contains("node runtime missing"));
    }

    #[test]
    fn runtime_doctor_warns_for_unavailable_required_runtimes() {
        let statuses = vec![
            runtime_status(ConversationAdapterRuntimeKind::Node, false, Some(">=20")),
            runtime_status(ConversationAdapterRuntimeKind::Python, true, None),
            runtime_status(ConversationAdapterRuntimeKind::Bash, true, None),
        ];

        let (status, message) = conversation_runtime_doctor_summary(&statuses);

        assert_eq!(status, "warn");
        assert!(message.contains("missing required adapter runtimes"));
        assert!(message.contains("node >=20"));
    }

    fn runtime_status(
        kind: ConversationAdapterRuntimeKind,
        available: bool,
        required_version: Option<&str>,
    ) -> ConversationAdapterRuntimeStatus {
        ConversationAdapterRuntimeStatus {
            kind,
            program: "runtime".to_string(),
            available,
            version: None,
            required_version: required_version.map(str::to_string),
            error: None,
            hint: None,
        }
    }
}
