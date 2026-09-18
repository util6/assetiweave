use super::*;
use crate::backend::{agent_market::types::AgentMarketErrorView, extension_kernel::ExtensionError};

#[test]
fn agent_market_error_preserves_structured_extension_details() {
    let app_error = AppError::from(ExtensionError::OutputLimitExceeded {
        package_id: "private-agent".to_string(),
        stdout: true,
        stderr: false,
    });
    let error = market_error_from_app(&app_error);
    let view = AgentMarketErrorView::from(&error);

    assert_eq!(view.code, "output_limit_exceeded");
    assert!(!view.retryable);
    assert_eq!(view.details.as_ref().unwrap()["stdout"], true);
    assert_eq!(view.details.as_ref().unwrap()["stderr"], false);
}

#[tokio::test]
async fn agent_market_error_parity_asserts_all_five_scenarios() {
    use crate::adapters::engine::transport::EngineError;
    use std::error::Error;

    // --- Scenario 1: SQL failure in Agent Market repository ---
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    let repo = crate::backend::agent_market::AgentInstallationRepository::new(pool);
    let repo_err = repo.get("test_agent").await.unwrap_err();
    let app_err = AppError::from(repo_err);
    assert_eq!(
        app_err.code(),
        "storage_error",
        "SQL failure must map to storage_error"
    );
    assert!(app_err.retryable(), "storage errors must be retryable");

    let mut found_sqlx = false;
    let mut cur: Option<&(dyn Error + 'static)> = app_err.source();
    while let Some(e) = cur {
        if e.is::<sqlx::Error>() {
            found_sqlx = true;
            break;
        }
        cur = e.source();
    }
    assert!(found_sqlx, "AppError source chain must contain sqlx::Error");

    let tauri_view = app_err.view();
    let engine_err = EngineError::from_app(app_err);
    assert_eq!(tauri_view.code, "storage_error");
    assert_eq!(engine_err.code, "storage_error");
    assert_eq!(tauri_view.retryable, engine_err.retryable);
    assert_eq!(tauri_view.message, engine_err.message);
    assert!(!tauri_view.message.to_ascii_lowercase().contains("select"));

    // --- Scenario 2: Illegal / invalid catalog ---
    let catalog_err = AgentMarketError::CatalogValidation {
        message: "Catalog version 99.0.0 unsupported".to_string(),
        agent_id: Some("agent_invalid".to_string()),
        field: Some("catalogVersion".to_string()),
        details: None,
    };
    let app_err = AppError::from(catalog_err);
    assert_eq!(app_err.code(), "catalog_validation_failed");
    assert!(!app_err.retryable());
    let tauri_view = app_err.view();
    let engine_err = EngineError::from_app(app_err);
    assert_eq!(tauri_view.code, engine_err.code);
    assert_eq!(tauri_view.message, engine_err.message);
    assert_eq!(tauri_view.retryable, engine_err.retryable);
    assert_eq!(tauri_view.details, engine_err.details);
    assert_eq!(
        tauri_view.details.as_ref().unwrap()["agentId"],
        "agent_invalid"
    );
    assert_eq!(
        tauri_view.details.as_ref().unwrap()["field"],
        "catalogVersion"
    );

    // --- Scenario 3: Missing installation ---
    let missing_err = AgentMarketError::InstallationNotFound {
        agent_id: "missing_agent_404".to_string(),
    };
    let app_err = AppError::from(missing_err);
    assert_eq!(app_err.code(), "agent_not_installed");
    assert!(!app_err.retryable());
    let tauri_view = app_err.view();
    let engine_err = EngineError::from_app(app_err);
    assert_eq!(tauri_view.code, engine_err.code);
    assert_eq!(tauri_view.message, engine_err.message);
    assert_eq!(tauri_view.retryable, engine_err.retryable);
    assert_eq!(tauri_view.details, engine_err.details);
    assert_eq!(
        tauri_view.details.as_ref().unwrap()["agentId"],
        "missing_agent_404"
    );

    // --- Scenario 4: Artifact / distribution mismatch ---
    let dist_err = AgentMarketError::Distribution {
        code: "distribution_artifact_mismatch".to_string(),
        message: "Checksum mismatch for distribution artifact".to_string(),
        agent_id: Some("agent_hash_fail".to_string()),
        distribution_id: Some("dist_darwin_arm64".to_string()),
        details: None,
    };
    let app_err = AppError::from(dist_err);
    assert_eq!(app_err.code(), "distribution_artifact_mismatch");
    assert!(!app_err.retryable());
    let tauri_view = app_err.view();
    let engine_err = EngineError::from_app(app_err);
    assert_eq!(tauri_view.code, engine_err.code);
    assert_eq!(tauri_view.message, engine_err.message);
    assert_eq!(tauri_view.retryable, engine_err.retryable);
    assert_eq!(tauri_view.details, engine_err.details);
    assert_eq!(
        tauri_view.details.as_ref().unwrap()["agentId"],
        "agent_hash_fail"
    );
    assert_eq!(
        tauri_view.details.as_ref().unwrap()["distributionId"],
        "dist_darwin_arm64"
    );

    // --- Scenario 5: Process timeout ---
    let proc_timeout_err =
        AgentMarketError::Process(crate::backend::host_process::HostProcessError::Timeout {
            stdout: vec![],
            stderr: vec![],
            stdout_truncated: false,
            stderr_truncated: false,
        });
    let app_err = AppError::from(proc_timeout_err);
    assert_eq!(app_err.code(), "timeout");
    assert!(app_err.retryable());
    let tauri_view = app_err.view();
    let engine_err = EngineError::from_app(app_err);
    assert_eq!(tauri_view.code, engine_err.code);
    assert_eq!(tauri_view.message, engine_err.message);
    assert_eq!(tauri_view.retryable, engine_err.retryable);
}
