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
fn test_skill_builtin_list() {
    // 1. Table format
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    let assert_table = cmd.args(["skill", "builtin", "list"]).assert().success();
    let table_out = String::from_utf8(assert_table.get_output().stdout.clone()).unwrap();
    assert!(table_out.contains("assetiweave-memory"));
    assert!(table_out.contains("assetiweave-conversation-organizer"));

    // 2. JSON format
    let mut cmd_json = Command::cargo_bin("aiwc").unwrap();
    let assert_json = cmd_json
        .args(["skill", "builtin", "list", "--format", "json"])
        .assert()
        .success();
    let json_out = String::from_utf8(assert_json.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&json_out).unwrap();
    assert_eq!(json["ok"], true);
    let arr = json["data"].as_array().expect("data array");
    assert!(!arr.is_empty());
    let memory_skill = arr
        .iter()
        .find(|item| item["name"] == "assetiweave-memory")
        .expect("assetiweave-memory in builtin list");
    assert_eq!(memory_skill["id"], "assetiweave.memory");
    assert!(!memory_skill["description"].as_str().unwrap().is_empty());
}

#[test]
fn test_skill_builtin_read() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    let assert_read = cmd
        .args(["skill", "builtin", "read", "assetiweave-memory"])
        .assert()
        .success();
    let stdout = String::from_utf8(assert_read.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("name: assetiweave-memory"));
    assert!(stdout.contains("AssetIWeave Memory"));
}

#[test]
fn test_skill_read_alias() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    let assert_read = cmd
        .args(["skill", "read", "assetiweave-conversation-organizer"])
        .assert()
        .success();
    let stdout = String::from_utf8(assert_read.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("name: assetiweave-conversation-organizer"));
    assert!(stdout.contains("AssetIWeave 对话记录整理"));

    let mut cmd_sk = Command::cargo_bin("aiwc").unwrap();
    let assert_sk = cmd_sk
        .args(["sk", "read", "assetiweave-memory"])
        .assert()
        .success();
    let stdout_sk = String::from_utf8(assert_sk.get_output().stdout.clone()).unwrap();
    assert!(stdout_sk.contains("name: assetiweave-memory"));
}

#[test]
fn test_skill_read_invalid_name() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    let assert_err = cmd
        .args(["skill", "read", "non-existent-skill-xyz"])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert_err.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("non-existent-skill-xyz"));

    let mut cmd_builtin = Command::cargo_bin("aiwc").unwrap();
    let assert_builtin_err = cmd_builtin
        .args(["skill", "builtin", "read", "non-existent-skill-xyz"])
        .assert()
        .failure();
    let stderr_builtin = String::from_utf8(assert_builtin_err.get_output().stderr.clone()).unwrap();
    assert!(stderr_builtin.contains("non-existent-skill-xyz"));
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
fn test_conversation_outline_help() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd
        .args(["conversation", "outline", "--help"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(output.contains("outline"));
}

#[test]
fn test_conversation_list_command() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    cmd.args(["conversation", "list", "--limit", "5"])
        .assert()
        .success();

    let mut cmd_json = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd_json
        .args(["conversation", "list", "--limit", "3", "--format", "json"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["data"].is_array());

    let mut cmd_window = Command::cargo_bin("aiwc").unwrap();
    cmd_window
        .args(["conversation", "list", "--window", "24h", "--limit", "5"])
        .assert()
        .success();

    let mut cmd_range = Command::cargo_bin("aiwc").unwrap();
    cmd_range
        .args([
            "conversation",
            "list",
            "--range",
            "2026-10-13..2026-10-20",
            "--limit",
            "5",
        ])
        .assert()
        .success();

    let mut cmd_since_dt = Command::cargo_bin("aiwc").unwrap();
    cmd_since_dt
        .args([
            "conversation",
            "list",
            "--since",
            "2026-10-13 18:52:16",
            "--until",
            "2026-10-20",
            "--limit",
            "5",
        ])
        .assert()
        .success();

    // Unified --time flag tests:
    // 1. Single integer (hours)
    let mut cmd_time_hours = Command::cargo_bin("aiwc").unwrap();
    cmd_time_hours
        .args(["conversation", "list", "--time", "24", "--limit", "5"])
        .assert()
        .success();

    // 2. Single datetime (A to now)
    let mut cmd_time_since = Command::cargo_bin("aiwc").unwrap();
    cmd_time_since
        .args([
            "conversation",
            "list",
            "--time",
            "2026-10-13",
            "--limit",
            "5",
        ])
        .assert()
        .success();

    // 3. Two datetimes (endpoints A and B)
    let mut cmd_time_two = Command::cargo_bin("aiwc").unwrap();
    let assert_time = cmd_time_two
        .args([
            "conversation",
            "list",
            "--time",
            "2026-10-13",
            "2026-10-20",
            "--limit",
            "5",
        ])
        .assert()
        .success();
    let output_table = String::from_utf8(assert_time.get_output().stdout.clone()).unwrap();
    if !output_table.contains("No conversation sessions found.") {
        assert!(output_table.contains("PROMPT (ENTRY -> LATEST IN WINDOW)"));
        assert!(output_table.contains("WINDOW SPAN"));
    }

    // 4. Verify JSON output contains window turn fields
    let mut cmd_time_json = Command::cargo_bin("aiwc").unwrap();
    let assert_json = cmd_time_json
        .args([
            "conversation",
            "list",
            "--time",
            "24",
            "--limit",
            "3",
            "--format",
            "json",
        ])
        .assert()
        .success();
    let output_json = String::from_utf8(assert_json.get_output().stdout.clone()).unwrap();
    let json_val: Value = serde_json::from_str(&output_json).unwrap();
    assert_eq!(json_val["ok"], true);
    if let Some(arr) = json_val["data"].as_array() {
        if let Some(first) = arr.first() {
            assert!(first.get("window_turn_count").is_some());
            assert!(first.get("window_start_turn_index").is_some());
            assert!(first.get("window_end_turn_index").is_some());
        }
    }
}

#[test]
fn test_conversation_outline_and_card_help() {
    let mut cmd_outline = Command::cargo_bin("aiwc").unwrap();
    let assert_outline = cmd_outline
        .args(["conversation", "outline", "--help"])
        .assert()
        .success();
    let out_outline = String::from_utf8(assert_outline.get_output().stdout.clone()).unwrap();
    assert!(out_outline.contains("outline"));

    let mut cmd_card = Command::cargo_bin("aiwc").unwrap();
    let assert_card = cmd_card
        .args(["conversation", "card", "--help"])
        .assert()
        .success();
    let out_card = String::from_utf8(assert_card.get_output().stdout.clone()).unwrap();
    assert!(out_card.contains("get"));

    let mut cmd_card_get = Command::cargo_bin("aiwc").unwrap();
    let assert_card_get = cmd_card_get
        .args(["conversation", "card", "get", "--help"])
        .assert()
        .success();
    let out_card_get = String::from_utf8(assert_card_get.get_output().stdout.clone()).unwrap();
    assert!(out_card_get.contains("--raw"));
}

#[test]
fn test_conversation_outline_and_card_drilldown() {
    let mut cmd_list = Command::cargo_bin("aiwc").unwrap();
    let assert_list = cmd_list
        .args(["conversation", "list", "--limit", "1", "--format", "json"])
        .assert()
        .success();
    let output = String::from_utf8(assert_list.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);

    let Some(arr) = json["data"].as_array() else {
        return;
    };
    let Some(first) = arr.first() else {
        return;
    };
    let session_id = first["short_id"].as_str().unwrap();

    // 1. Session outline via `conversation outline <id>`
    let mut cmd_so = Command::cargo_bin("aiwc").unwrap();
    let assert_so = cmd_so
        .args(["conversation", "outline", session_id])
        .assert()
        .success();
    let out_so = String::from_utf8(assert_so.get_output().stdout.clone()).unwrap();
    let json_so: Value = serde_json::from_str(&out_so).unwrap();
    assert_eq!(json_so["ok"], true);
    assert_eq!(json_so["data"]["level"], "session");
    assert!(json_so["data"]["turns"]["columns"].is_array());
    assert!(json_so["data"]["turns"]["rows"].is_array());

    // If turns exist in the session, drill down to turn outline
    let rows = json_so["data"]["turns"]["rows"].as_array().unwrap();
    if let Some(first_row) = rows.first() {
        let turn_id = first_row[1].as_str().unwrap();

        // 3. Turn outline via `conversation outline <turn_id>`
        let mut cmd_to = Command::cargo_bin("aiwc").unwrap();
        let assert_to = cmd_to
            .args(["conversation", "outline", turn_id])
            .assert()
            .success();
        let out_to = String::from_utf8(assert_to.get_output().stdout.clone()).unwrap();
        let json_to: Value = serde_json::from_str(&out_to).unwrap();
        assert_eq!(json_to["ok"], true);
        assert_eq!(json_to["data"]["level"], "turn");
        assert!(json_to["data"]["cards"]["columns"].is_array());
        assert!(json_to["data"]["cards"]["rows"].is_array());

        // If cards exist in turn, test card outline and card get
        let card_rows = json_to["data"]["cards"]["rows"].as_array().unwrap();
        if let Some(first_card_row) = card_rows.first() {
            let card_id = first_card_row[1].as_str().unwrap();

            // 4. Card outline via `conversation outline <card_id>`
            let mut cmd_co = Command::cargo_bin("aiwc").unwrap();
            let assert_co = cmd_co
                .args(["conversation", "outline", card_id])
                .assert()
                .success();
            let out_co = String::from_utf8(assert_co.get_output().stdout.clone()).unwrap();
            let json_co: Value = serde_json::from_str(&out_co).unwrap();
            assert_eq!(json_co["ok"], true);
            assert_eq!(json_co["data"]["level"], "card");
            assert_eq!(json_co["data"]["target"]["card_id"], card_id);
            assert!(json_co["data"]["neighborhood"]["rows"].is_array());

            // 5. Card get JSON via `conversation card get <card_id>`
            let mut cmd_cg = Command::cargo_bin("aiwc").unwrap();
            let assert_cg = cmd_cg
                .args(["conversation", "card", "get", card_id])
                .assert()
                .success();
            let out_cg = String::from_utf8(assert_cg.get_output().stdout.clone()).unwrap();
            let json_cg: Value = serde_json::from_str(&out_cg).unwrap();
            assert_eq!(json_cg["ok"], true);
            assert_eq!(json_cg["data"]["card_id"], card_id);
            let content = json_cg["data"]["content"].as_str().unwrap();

            // 6. Card get raw via `conversation card get <card_id> --raw`
            let mut cmd_cgr = Command::cargo_bin("aiwc").unwrap();
            let assert_cgr = cmd_cgr
                .args(["conversation", "card", "get", card_id, "--raw"])
                .assert()
                .success();
            let out_cgr = String::from_utf8(assert_cgr.get_output().stdout.clone()).unwrap();
            assert_eq!(out_cgr.trim(), content.trim());
        }
    }
}
