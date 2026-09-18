use super::*;

fn sanitize_log_value(value: &str) -> String {
    sanitize_log_text(value).replace('"', "\\\"")
}

#[test]
fn sanitize_log_helpers_escape_properly() {
    assert_eq!(
        sanitize_log_text("挂载失败\n需要查看异常"),
        "挂载失败\\n需要查看异常"
    );
    assert_eq!(sanitize_log_key("skill mount!"), "skill_mount");
    assert_eq!(
        sanitize_log_value("path contains \"target\""),
        "path contains \\\"target\\\""
    );
}

#[test]
fn panic_log_is_available_through_the_log_viewer() {
    assert!(is_managed_log_file_name("panic.log"));
    assert!(is_managed_log_file_name("panic.log.1"));
}

#[test]
fn panic_log_writer_uses_the_next_path_when_primary_path_fails() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-panic-log-test-{}",
        std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time after epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create panic log test root");
    let blocking_parent = root.join("blocking-parent");
    fs::write(&blocking_parent, "not a directory").expect("create blocking parent");
    let primary = blocking_parent.join("panic.log");
    let fallback = root.join("fallback").join("panic.log");

    write_fatal_panic_log(&[primary, fallback.clone()], "panic details")
        .expect("write fallback panic log");

    assert!(fs::read_to_string(fallback)
        .expect("read fallback panic log")
        .contains("panic details"));
    fs::remove_dir_all(root).expect("remove panic log test root");
}

#[test]
fn write_operation_command_validates_level_and_accepts_payload() {
    assert!(logs_write_operation(
        "INFO".to_string(),
        "source.create".to_string(),
        "添加数据来源成功".to_string(),
        Some(BTreeMap::from([
            ("source_id".to_string(), "source-a".to_string()),
            ("root_path".to_string(), "/tmp/skills".to_string()),
        ])),
    )
    .is_ok());

    assert!(logs_write_operation(
        "UNKNOWN".to_string(),
        "source.create".to_string(),
        "添加数据来源成功".to_string(),
        None,
    )
    .is_err());
}

#[test]
fn ordinary_logs_no_longer_open_file_for_each_event() {
    let source = include_str!("logs.rs");
    assert!(!source.contains(concat!("fn append_app_", "log_line(")));
    assert!(!source.contains(concat!("fn format_operation_", "log_line(")));
}

#[test]
fn resolve_managed_log_file_rejects_path_escape_and_missing_file() {
    let _ = ensure_default_log_file();

    let escape_err = resolve_managed_log_file(Some("../../etc/passwd")).unwrap_err();
    assert!(matches!(escape_err, LogAccessError::PathEscape(_)));

    let slash_err = resolve_managed_log_file(Some("foo/bar.log")).unwrap_err();
    assert!(matches!(slash_err, LogAccessError::PathEscape(_)));

    let not_found_err = resolve_managed_log_file(Some("non_existent_file_xyz.log")).unwrap_err();
    assert!(matches!(not_found_err, LogAccessError::FileNotFound(_)));
}

#[test]
fn write_operation_typed_error_on_invalid_level() {
    let err = logs_write_operation(
        "INVALID_LEVEL".to_string(),
        "op".to_string(),
        "msg".to_string(),
        None,
    )
    .unwrap_err();
    assert!(matches!(err, LogAccessError::InvalidLogLevel(_)));
}
