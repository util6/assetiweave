use assert_cmd::Command;
use serde_json::Value;

#[test]
fn test_version_command() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    cmd.arg("version").assert().success();

    let mut cmd_json = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd_json
        .args(["version", "--format", "json"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["compatible"], true);
}

#[test]
fn test_overview_command() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    cmd.arg("overview").assert().success();

    let mut cmd_json = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd_json
        .args(["overview", "--format", "json"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["data"]["source_count"].is_number());
}

#[test]
fn test_shortcuts_aliases() {
    // aiwc ov -> overview
    let mut cmd_ov = Command::cargo_bin("aiwc").unwrap();
    cmd_ov.arg("ov").assert().success();

    // aiwc sk list -> skill list
    let mut cmd_sk = Command::cargo_bin("aiwc").unwrap();
    cmd_sk.args(["sk", "list"]).assert().success();

    // aiwc c adapter list -> conversation adapter list
    let mut cmd_c = Command::cargo_bin("aiwc").unwrap();
    cmd_c.args(["c", "adapter", "list"]).assert().success();
}

#[test]
fn test_doctor_command() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd.args(["doctor", "--format", "json"]).assert().success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["data"]["checks"].is_array());
}

#[test]
fn test_settings_show() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd
        .args(["settings", "show", "--format", "json"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["data"]["settings"].is_object());
}

#[test]
fn test_harvester_template_list() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd
        .args(["harvester", "template", "list", "--format", "json"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["data"]["templates"].is_array());
}

#[test]
fn test_schema_get() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd
        .args(["schema", "overview.get", "--format", "json"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["method"], "overview.get");
}

#[test]
fn test_conversation_adapter_list() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd
        .args(["conversation", "adapter", "list", "--format", "json"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["data"].is_array());
}

#[test]
fn test_skill_list() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    cmd.args(["skill", "list"]).assert().success();

    let mut cmd_json = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd_json
        .args(["skill", "list", "--format", "json"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["data"].is_array());
}

#[test]
fn test_source_list() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    cmd.args(["source", "list"]).assert().success();

    let mut cmd_json = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd_json
        .args(["source", "list", "--format", "json"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["data"].is_array());
}

#[test]
fn test_tenant_list() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    cmd.args(["tenant", "list"]).assert().success();

    let mut cmd_json = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd_json
        .args(["tenant", "list", "--format", "json"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["data"].is_array());
}

#[test]
fn test_profile_list() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    cmd.args(["profile", "list"]).assert().success();

    let mut cmd_alias = Command::cargo_bin("aiwc").unwrap();
    cmd_alias
        .args(["target-profile", "list"])
        .assert()
        .success();

    let mut cmd_json = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd_json
        .args(["profile", "list", "--format", "json"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["data"].is_array());
}

#[test]
fn test_config_plugins_show() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd
        .args(["config", "plugins", "show", "--format", "json"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["data"]["plugins"].is_array());
}

#[test]
fn test_memory_recent_get() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd
        .args(["memory", "recent", "get", "--format", "json"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
}

#[test]
fn test_completion() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd.args(["completion", "bash"]).assert().success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(output.contains("_aiwc"));
}

#[test]
fn test_conversation_session_help() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd
        .args(["conversation", "session", "--help"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(output.contains("outline"));
    assert!(output.contains("get"));
}
