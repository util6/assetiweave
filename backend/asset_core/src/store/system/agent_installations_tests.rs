use std::time::SystemTime;

use super::*;
use crate::backend::store::Database;

#[tokio::test]
async fn repository_is_application_scoped_and_upsert_keeps_one_current_row() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-agent-repo-{}.db",
        uuid::Uuid::new_v4()
    ));
    let database = Database::open_initialized_async(&path)
        .await
        .expect("database");
    let repository = AgentInstallationRepository::new(database.pool().clone());
    let now = format!("{:?}", SystemTime::now());
    let installation = fixture("agent", "installation", &now);
    repository
        .upsert_active(&installation)
        .await
        .expect("upsert");
    repository
        .upsert_active(&AgentInstallation {
            installation_id: "replacement".to_string(),
            ..installation.clone()
        })
        .await
        .expect("replacement");
    let rows = repository.list().await.expect("list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].installation_id, "replacement");

    // Verify app_extension_versions has recorded the version
    let versions_count: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM app_extension_versions WHERE extension_id = 'agent' AND version = '1.0.0'",
    )
    .fetch_one(database.pool())
    .await
    .expect("query version");
    assert_eq!(versions_count.0, 1);

    // Test health update with json_set
    let mut updated_health = rows[0].clone();
    updated_health.runtime_status = RuntimeStatus::Failed;
    updated_health.runtime_error_code = Some("health_err".to_string());
    updated_health.runtime_error_message = Some("Health failed".to_string());
    repository
        .update_health(&updated_health)
        .await
        .expect("update health");

    let fetched = repository.get("agent").await.expect("get").unwrap();
    assert_eq!(fetched.runtime_status, RuntimeStatus::Failed);
    assert_eq!(fetched.runtime_error_code.as_deref(), Some("health_err"));

    // Test mark_broken
    repository
        .mark_broken(
            "agent",
            RuntimeStatus::Failed,
            "broken_code",
            "broken message",
            &now,
        )
        .await
        .expect("mark broken");
    let fetched = repository.get("agent").await.expect("get").unwrap();
    assert_eq!(fetched.installation_status, InstallationStatus::Broken);
    assert_eq!(fetched.runtime_error_code.as_deref(), Some("broken_code"));

    // Test mark_incompatible
    repository
        .mark_incompatible("agent", "agent_reinstall_required", "reinstall", &now)
        .await
        .expect("mark incompatible");
    let fetched = repository.get("agent").await.expect("get").unwrap();
    assert_eq!(
        fetched.installation_status,
        InstallationStatus::Incompatible
    );
    assert_eq!(
        fetched.runtime_error_code.as_deref(),
        Some("agent_reinstall_required")
    );

    // Test delete removes both app_extensions and app_extension_versions
    repository.delete("agent").await.expect("delete");
    assert!(repository.get("agent").await.expect("get").is_none());
    let v_count: (i64,) =
        sqlx::query_as("SELECT count(*) FROM app_extension_versions WHERE extension_id = 'agent'")
            .fetch_one(database.pool())
            .await
            .expect("query version");
    assert_eq!(v_count.0, 0);

    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn agent_installation_dual_insurance_folds_on_write_and_expands_on_read() {
    let path = std::env::temp_dir().join(format!(
        "assetiweave-agent-paths-{}.db",
        uuid::Uuid::new_v4()
    ));
    let database = Database::open_initialized_async(&path)
        .await
        .expect("database");
    let repository = AgentInstallationRepository::new(database.pool().clone());
    let now = format!("{:?}", SystemTime::now());

    let home_dir = dirs::home_dir().expect("home dir");
    let target_install_dir = home_dir.join(".assetiweave/agent-runtimes/active/test-agent");
    let target_program = target_install_dir.join("test-bin");

    let mut installation = fixture("dual-insurance-agent", "inst-1", &now);
    installation.install_dir = Some(target_install_dir.clone());
    installation.resolved_program = target_program.clone();
    installation.definition_json = serde_json::json!({
        "program": target_program.to_string_lossy(),
        "args": ["run"]
    });

    repository
        .upsert_active(&installation)
        .await
        .expect("upsert active");

    // 1. 验证存入数据库时被强制折叠为 '~'
    let row: (Option<String>, String) = sqlx::query_as(
        "SELECT install_dir, capability_meta_json FROM app_extensions WHERE extension_id = 'dual-insurance-agent'",
    )
    .fetch_one(database.pool())
    .await
    .expect("query raw row");

    assert_eq!(
        row.0.as_deref(),
        Some("~/.assetiweave/agent-runtimes/active/test-agent"),
        "install_dir in app_extensions must be folded to '~'"
    );
    assert!(
        row.1.contains(
            r#""resolved_program":"~/.assetiweave/agent-runtimes/active/test-agent/test-bin""#
        ),
        "capability_meta_json must contain folded resolved_program"
    );

    let ver_row: (String,) = sqlx::query_as(
        "SELECT install_dir FROM app_extension_versions WHERE extension_id = 'dual-insurance-agent' AND version = '1.0.0'",
    )
    .fetch_one(database.pool())
    .await
    .expect("query version raw row");

    assert_eq!(
        ver_row.0, "~/.assetiweave/agent-runtimes/active/test-agent",
        "install_dir in app_extension_versions must be folded to '~'"
    );

    // 2. 验证读出时自动展开为完整的物理绝对路径
    let fetched = repository
        .get("dual-insurance-agent")
        .await
        .expect("get")
        .expect("must exist");

    assert_eq!(
        fetched.install_dir.as_ref(),
        Some(&target_install_dir),
        "read install_dir must be expanded back to full absolute PathBuf"
    );
    assert_eq!(
        fetched.resolved_program, target_program,
        "read resolved_program must be expanded back to full absolute PathBuf"
    );
    assert_eq!(
        fetched
            .definition_json
            .get("program")
            .and_then(|v| v.as_str()),
        Some(target_program.to_str().unwrap()),
        "read definition_json program must be expanded"
    );

    // 3. 验证防御读取：即便库中存在误存的绝对路径（旧数据残留），读出时依然正常识别解析
    sqlx::query(
        "UPDATE app_extensions SET install_dir = '/tmp/custom/legacy/path' WHERE extension_id = 'dual-insurance-agent'",
    )
    .execute(database.pool())
    .await
    .expect("update legacy un-folded path");

    let legacy_fetched = repository
        .get("dual-insurance-agent")
        .await
        .expect("get legacy")
        .expect("must exist");

    assert_eq!(
        legacy_fetched.install_dir.as_ref(),
        Some(&PathBuf::from("/tmp/custom/legacy/path")),
        "legacy un-folded absolute path must still resolve gracefully"
    );

    let _ = std::fs::remove_file(path);
}

fn fixture(agent_id: &str, installation_id: &str, now: &str) -> AgentInstallation {
    AgentInstallation {
        agent_id: agent_id.to_string(),
        installation_id: installation_id.to_string(),
        display_name: "Agent".to_string(),
        catalog_item_version: "1.0.0".to_string(),
        agent_version: "1.0.0".to_string(),
        protocol: AgentMarketProtocol::Acp,
        distribution_id: "binary".to_string(),
        distribution_type: DistributionType::Binary,
        ownership: Ownership::System,
        install_dir: None,
        resolved_program: PathBuf::from("/usr/bin/agent"),
        args: Vec::new(),
        definition_json: serde_json::json!({}),
        integrity_json: None,
        source_registry: "agent".to_string(),
        catalog_version: "2026.08.16.1".to_string(),
        enabled: true,
        installation_status: InstallationStatus::Ready,
        runtime_status: RuntimeStatus::Ready,
        runtime_error_code: None,
        runtime_error_message: None,
        runtime_checked_at: None,
        protocol_status: ProtocolStatus::Ready,
        protocol_error_code: None,
        protocol_error_message: None,
        protocol_checked_at: None,
        model_status: None,
        model_error_code: None,
        model_checked_at: None,
        installed_at: now.to_string(),
        updated_at: now.to_string(),
    }
}
