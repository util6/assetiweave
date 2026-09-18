use super::*;

#[test]
fn application_error_preserves_validation_code_across_tauri_view() {
    let view = AppError::Validation("bad input".to_string()).view();

    assert_eq!(view.code, "validation_error");
    assert!(!view.retryable);
    assert_eq!(view.message, "bad input");
}

#[test]
fn external_error_never_exposes_debug_payload() {
    let value = serde_json::to_value(AppError::External("plain failure".to_string()))
        .expect("wire error serializes");
    assert_eq!(value["code"], "external_error");
    assert!(value.get("AppError").is_none());
    assert!(value.get("Legacy").is_none());
}

#[test]
fn infrastructure_wire_error_hides_internal_diagnostics() {
    let view = AppError::External("SQL /Users/util6/private.db token=secret".to_string()).view();

    assert_eq!(view.code, "external_error");
    assert_eq!(view.message, "An external operation failed.");
    assert!(!view.message.contains("/Users/util6"));
    assert!(!view.message.contains("secret"));
}

#[test]
fn external_helper_preserves_explicit_boundary_mapping() {
    let error = AppError::external("plain failure");

    assert_eq!(error.code(), "external_error");
    assert!(error.retryable());
}

#[test]
fn domain_wire_error_redacts_sensitive_message_and_details() {
    let error = AppError::Domain {
        code: "fixture_failed".to_string(),
        message: "failed at /Users/util6/private.db token=secret".to_string(),
        retryable: true,
        details: Some(serde_json::json!({
            "path": "/Users/util6/private.db",
            "token": "secret",
            "phase": "prompting",
        })),
    };

    let view = error.view();

    assert_eq!(view.message, "The operation failed.");
    let details = view.details.as_ref().expect("safe details");
    assert!(details.get("token").is_none());
    assert_eq!(details["path"], "<redacted>");
    assert_eq!(details["phase"], "prompting");
    assert!(!serde_json::to_string(&view).unwrap().contains("secret"));
    assert!(!serde_json::to_string(&view)
        .unwrap()
        .contains("/Users/util6"));
}

#[test]
fn io_error_keeps_source_while_wire_message_is_sanitized() {
    use std::error::Error;
    let error = AppError::from(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "/private/token-file",
    ));
    assert!(error.source().is_some());
    let wire = error.view();
    assert_eq!(wire.code, "storage_error");
    assert!(!wire.message.contains("token-file"));
}

#[test]
fn ai_execution_error_uses_derive_instead_of_manual_error_impl() {
    let source = include_str!("../ai_execution/error.rs");
    assert!(!source.contains(concat!("impl fmt::Display for ", "AiExecutionError")));
    assert!(source.contains("thiserror::Error"));
}

#[test]
fn validation_error_maps_controlled_field_codes_without_leaking_params() {
    use validator::ValidationError;
    let mut err = ValidationError::new("length_bytes");
    err.add_param(std::borrow::Cow::Borrowed("secret"), &"token=12345");
    let mut errors = validator::ValidationErrors::new();
    errors.add("display_name", err);

    let app_err = validation_error(errors);
    let wire = app_err.view();
    assert_eq!(wire.code, "validation_error");
    assert_eq!(
        wire.message,
        "validation failed: display_name: length_bytes"
    );
    assert!(!wire.message.contains("12345"));
    assert!(!wire.message.contains("token"));
}

#[test]
fn cancellation_wire_parity_asserts_code_retryable_and_safe_message() {
    let err = AppError::Cancelled("sensitive internal reason token=123".to_string());
    assert_eq!(err.code(), "cancelled");
    assert!(err.retryable());
    let view = err.view();
    assert_eq!(view.code, "cancelled");
    assert_eq!(view.message, "The operation was cancelled.");
    assert!(view.retryable);
    assert_eq!(view.details, None);

    let json = serde_json::to_value(&view).expect("serializes to wire error");
    assert_eq!(json["code"], "cancelled");
    assert_eq!(json["message"], "The operation was cancelled.");
    assert_eq!(json["retryable"], true);
    assert!(json["details"].is_null());
    assert!(!serde_json::to_string(&json).unwrap().contains("sensitive"));
    assert!(!serde_json::to_string(&json).unwrap().contains("token"));
}

#[test]
fn host_process_cancellation_maps_to_cancelled_wire_parity() {
    let host_err = crate::backend::host_process::HostProcessError::Cancelled;
    let app_err: AppError = host_err.into();
    assert!(matches!(app_err, AppError::Cancelled(_)));
    assert_eq!(app_err.code(), "cancelled");
    assert!(app_err.retryable());
    let view = app_err.view();
    assert_eq!(view.code, "cancelled");
    assert_eq!(view.message, "The operation was cancelled.");
    assert!(view.retryable);
}

#[tokio::test]
async fn task_runtime_join_cancellation_maps_to_cancelled_wire_parity() {
    let handle = tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    });
    handle.abort();
    let join_err = handle.await.unwrap_err();
    assert!(join_err.is_cancelled());
    let app_err = AppError::from(join_err);
    assert!(matches!(app_err, AppError::Cancelled(_)));
    assert_eq!(app_err.code(), "cancelled");
    assert!(app_err.retryable());
    let view = app_err.view();
    assert_eq!(view.code, "cancelled");
    assert_eq!(view.message, "The operation was cancelled.");
    assert!(view.retryable);
}

#[tokio::test]
async fn sqlx_row_error_preserves_database_source_and_wire_code() {
    use std::error::Error;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    let query_err = sqlx::query_as::<_, (i32,)>("SELECT 'not_an_integer' AS count")
        .fetch_one(&pool)
        .await
        .unwrap_err();
    let app_err = AppError::from(query_err);
    assert_eq!(app_err.code(), "storage_error");
    assert!(app_err.retryable());
    let mut found_sqlx = false;
    let mut cur: Option<&(dyn Error + 'static)> = app_err.source();
    while let Some(e) = cur {
        if e.is::<sqlx::Error>() {
            found_sqlx = true;
            break;
        }
        cur = e.source();
    }
    assert!(found_sqlx, "source chain must contain sqlx::Error");
    let view = app_err.view();
    assert_eq!(view.code, "storage_error");
    assert!(!view.message.to_ascii_lowercase().contains("select"));
    assert!(!view.message.contains("not_an_integer"));
}

#[test]
fn projection_and_log_error_wire_parity() {
    use crate::backend::logs::LogAccessError;
    use crate::backend::projection::error::ProjectionError;
    use std::error::Error;

    // 1. 未知 card schema (UnsupportedSchemaVersion) -> Validation, wire code validation_error, non-retryable
    let proj_schema_err = ProjectionError::UnsupportedSchemaVersion {
        expected: 1,
        actual: Some(999),
    };
    let app_err = AppError::from(proj_schema_err);
    assert!(matches!(app_err, AppError::Validation(_)));
    assert_eq!(app_err.code(), "validation_error");
    assert!(!app_err.retryable());
    let view = app_err.view();
    assert_eq!(view.code, "validation_error");
    assert!(!view.retryable);
    assert!(view.message.contains("schema_version"));
    assert!(view.message.contains("1"));

    // 2. 非法 renderer (UnsupportedRenderer) -> Validation, wire code validation_error, non-retryable
    let proj_renderer_err = ProjectionError::UnsupportedRenderer {
        renderer: "unknown_3d_canvas".to_string(),
    };
    let app_err = AppError::from(proj_renderer_err);
    assert!(matches!(app_err, AppError::Validation(_)));
    assert_eq!(app_err.code(), "validation_error");
    assert!(!app_err.retryable());
    let view = app_err.view();
    assert_eq!(view.code, "validation_error");
    assert!(!view.retryable);
    assert!(view.message.contains("unknown_3d_canvas"));

    // 3. 日志路径逃逸 (PathEscape) -> Validation, wire code validation_error, non-retryable
    let log_escape_err = LogAccessError::PathEscape("../../etc/passwd".to_string());
    let app_err = AppError::from(log_escape_err);
    assert!(matches!(app_err, AppError::Validation(_)));
    assert_eq!(app_err.code(), "validation_error");
    assert!(!app_err.retryable());
    let view = app_err.view();
    assert_eq!(view.code, "validation_error");
    assert!(!view.retryable);
    assert!(view.message.contains("非法日志路径访问"));

    // 4. 日志读取 I/O (Io) -> Io, wire code storage_error, retryable, 保留 std::io::Error source
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied");
    let log_io_err = LogAccessError::Io {
        action: "打开日志文件",
        path: Some(std::path::PathBuf::from("/var/log/app.log")),
        source: io_err,
    };
    let app_err = AppError::from(log_io_err);
    assert!(matches!(app_err, AppError::Io(_)));
    assert_eq!(app_err.code(), "storage_error");
    assert!(app_err.retryable());
    assert!(app_err.source().is_some(), "source must be preserved");
    let root_source = app_err.source().unwrap();
    assert!(root_source.is::<std::io::Error>());
    let view = app_err.view();
    assert_eq!(view.code, "storage_error");
    assert!(view.retryable);
    assert_eq!(
        view.message,
        "The application could not access local storage."
    );
}

#[test]
fn sanitization_redacts_sensitive_keywords_in_public_message_and_details() {
    use serde_json::json;

    // 1. 绝对路径 (Unix, Tilde, Windows)
    assert_eq!(
        sanitize_public_message("error at /var/data/users.json occurred"),
        "The operation failed."
    );
    assert_eq!(
        sanitize_public_message("error at ~/Documents/keys.pem occurred"),
        "The operation failed."
    );
    assert_eq!(
        sanitize_public_message("error at C:\\Users\\Admin\\config.ini occurred"),
        "The operation failed."
    );
    assert_eq!(
        sanitize_public_message("error at C:/Users/Admin/config.ini occurred"),
        "The operation failed."
    );

    // 2. SQL
    assert_eq!(
        sanitize_public_message("SQL error near SELECT * FROM users"),
        "The operation failed."
    );

    // 3. Token
    assert_eq!(
        sanitize_public_message("invalid bearer token=xyz123"),
        "The operation failed."
    );

    // 4. Secret
    assert_eq!(
        sanitize_public_message("leaked secret value in header"),
        "The operation failed."
    );

    // 5. Password
    assert_eq!(
        sanitize_public_message("invalid password provided for user"),
        "The operation failed."
    );

    // 6. Prompt
    assert_eq!(
        sanitize_public_message("invalid syntax in prompt=system_prompt"),
        "The operation failed."
    );
    assert_eq!(
        sanitize_public_message("missing prompt: user_input"),
        "The operation failed."
    );

    // 7. Environment
    assert_eq!(
        sanitize_public_message("failed to read environment variable PATH"),
        "The operation failed."
    );

    // 8. Safe messages are preserved
    assert_eq!(
        sanitize_public_message("file not found: item-42"),
        "file not found: item-42"
    );

    // 9. Details object sanitization
    let details = json!({
        "secret": "hidden123",
        "token": "tok456",
        "password": "pass",
        "prompt": "my prompt text",
        "environment": "production",
        "safe_field": "safe_value",
        "path_field": "/etc/shadow",
    });
    let sanitized = sanitize_details(&details).expect("sanitized object");
    assert!(sanitized.get("secret").is_none());
    assert!(sanitized.get("token").is_none());
    assert!(sanitized.get("password").is_none());
    assert!(sanitized.get("prompt").is_none());
    assert!(sanitized.get("environment").is_none());
    assert_eq!(sanitized["safe_field"], "safe_value");
    assert_eq!(sanitized["path_field"], "<redacted>");
}
