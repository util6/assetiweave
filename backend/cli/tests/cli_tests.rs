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
    assert!(stderr.contains("Skill not found: non-existent-skill-xyz"));
    assert!(stderr.contains("Hint: run 'aiwc skill builtin list'"));

    let mut cmd_builtin = Command::cargo_bin("aiwc").unwrap();
    let assert_builtin_err = cmd_builtin
        .args(["skill", "builtin", "read", "non-existent-skill-xyz"])
        .assert()
        .failure();
    let stderr_builtin = String::from_utf8(assert_builtin_err.get_output().stderr.clone()).unwrap();
    assert!(stderr_builtin.contains("Skill not found: non-existent-skill-xyz"));
    assert!(stderr_builtin.contains("Hint: run 'aiwc skill builtin list'"));
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

#[test]
fn test_conversation_affordance_long_help_and_concise_short_help() {
    // 1. Top-level short help `conversation -h` should be concise and contain no affordance
    let mut cmd_short = Command::cargo_bin("aiwc").unwrap();
    let assert_short = cmd_short.args(["conversation", "-h"]).assert().success();
    let out_short = String::from_utf8(assert_short.get_output().stdout.clone()).unwrap();
    assert!(out_short.contains("Usage: aiwc conversation"));
    assert!(!out_short.contains("Affordance:"));
    assert!(!out_short.contains("Use this when"));
    assert!(!out_short.contains("Avoid when"));

    // 2. Top-level long help `conversation --help` should contain 5-dimension affordance
    let mut cmd_long = Command::cargo_bin("aiwc").unwrap();
    let assert_long = cmd_long.args(["conversation", "--help"]).assert().success();
    let out_long = String::from_utf8(assert_long.get_output().stdout.clone()).unwrap();
    assert!(out_long.contains("Affordance:"));
    assert!(out_long.contains("Use this when:"));
    assert!(out_long.contains("Avoid when:"));
    assert!(out_long.contains("Examples:"));
    assert!(out_long.contains("Related skills:"));
    assert!(out_long.contains("Risk:"));
    assert!(out_long.contains("aiwc skill builtin read assetiweave-conversation-recall"));

    // 3. Subcommand short help `conversation search -h` should be concise
    let mut cmd_search_short = Command::cargo_bin("aiwc").unwrap();
    let assert_search_short = cmd_search_short
        .args(["conversation", "search", "-h"])
        .assert()
        .success();
    let out_search_short =
        String::from_utf8(assert_search_short.get_output().stdout.clone()).unwrap();
    assert!(out_search_short.contains("Usage: aiwc conversation search"));
    assert!(!out_search_short.contains("Affordance:"));
    assert!(!out_search_short.contains("Use this when"));

    // 4. Subcommand long help `conversation search --help` should contain 5-dimension affordance
    let mut cmd_search_long = Command::cargo_bin("aiwc").unwrap();
    let assert_search_long = cmd_search_long
        .args(["conversation", "search", "--help"])
        .assert()
        .success();
    let out_search_long =
        String::from_utf8(assert_search_long.get_output().stdout.clone()).unwrap();
    assert!(out_search_long.contains("Affordance:"));
    assert!(out_search_long.contains("Use this when:"));
    assert!(out_search_long.contains("Avoid when:"));
    assert!(out_search_long.contains("Examples:"));
    assert!(out_search_long.contains("Related skills:"));
    assert!(out_search_long.contains("Risk:"));
    assert!(out_search_long.contains("aiwc skill builtin read assetiweave-conversation-recall"));

    // 5. Verify other conversation subcommands have long help affordance
    for subcmd in [
        "outline", "card", "sync", "adapter", "source", "data", "list",
    ] {
        let mut cmd_sub = Command::cargo_bin("aiwc").unwrap();
        let assert_sub = cmd_sub
            .args(["conversation", subcmd, "--help"])
            .assert()
            .success();
        let out_sub = String::from_utf8(assert_sub.get_output().stdout.clone()).unwrap();
        assert!(
            out_sub.contains("Affordance:"),
            "subcommand {} missing Affordance header",
            subcmd
        );
        assert!(
            out_sub.contains("Use this when:"),
            "subcommand {} missing Use this when",
            subcmd
        );
        assert!(
            out_sub.contains("Avoid when:"),
            "subcommand {} missing Avoid when",
            subcmd
        );
        assert!(
            out_sub.contains("Examples:"),
            "subcommand {} missing Examples",
            subcmd
        );
        assert!(
            out_sub.contains("Related skills:"),
            "subcommand {} missing Related skills",
            subcmd
        );
        assert!(
            out_sub.contains("Risk:"),
            "subcommand {} missing Risk",
            subcmd
        );
    }
}

#[test]
fn test_conversation_error_actionable_hints_text_mode() {
    // 1. Outline not found
    let mut cmd_outline = Command::cargo_bin("aiwc").unwrap();
    let assert_outline = cmd_outline
        .args(["conversation", "outline", "non-existent-outline-id"])
        .assert()
        .failure();
    let stderr_outline = String::from_utf8(assert_outline.get_output().stderr.clone()).unwrap();
    assert!(
        stderr_outline.contains("Error: Conversation outline not found: non-existent-outline-id")
    );
    assert!(stderr_outline.contains("Hint: run 'aiwc conversation list'"));

    // 2. Card not found
    let mut cmd_card = Command::cargo_bin("aiwc").unwrap();
    let assert_card = cmd_card
        .args(["conversation", "card", "get", "non-existent-card-id"])
        .assert()
        .failure();
    let stderr_card = String::from_utf8(assert_card.get_output().stderr.clone()).unwrap();
    assert!(stderr_card.contains("Error: Conversation card not found: non-existent-card-id"));
    assert!(
        stderr_card.contains("Hint: run 'aiwc conversation outline' or 'aiwc conversation search'")
    );

    // 3. Adapter not found
    let mut cmd_adapter = Command::cargo_bin("aiwc").unwrap();
    let assert_adapter = cmd_adapter
        .args([
            "conversation",
            "adapter",
            "inspect",
            "non-existent-adapter-id",
        ])
        .assert()
        .failure();
    let stderr_adapter = String::from_utf8(assert_adapter.get_output().stderr.clone()).unwrap();
    assert!(
        stderr_adapter.contains("Error: Conversation adapter not found: non-existent-adapter-id")
    );
    assert!(stderr_adapter.contains("Hint: run 'aiwc conversation adapter list'"));

    // 4. Search missing query
    let mut cmd_search = Command::cargo_bin("aiwc").unwrap();
    let assert_search = cmd_search
        .args(["conversation", "search"])
        .assert()
        .failure();
    let stderr_search = String::from_utf8(assert_search.get_output().stderr.clone()).unwrap();
    assert!(stderr_search.contains("Error: Conversation search query is required"));
    assert!(stderr_search.contains(
        "Hint: run 'aiwc conversation search --query <QUERY>' or 'aiwc conversation list'"
    ));
}

#[test]
fn test_conversation_error_actionable_hints_json_mode() {
    // 1. Outline not found in JSON
    let mut cmd_outline = Command::cargo_bin("aiwc").unwrap();
    let assert_outline = cmd_outline
        .args([
            "conversation",
            "outline",
            "non-existent-outline-id",
            "--format",
            "json",
        ])
        .assert()
        .failure();
    let stdout_outline = String::from_utf8(assert_outline.get_output().stdout.clone()).unwrap();
    let json_outline: Value = serde_json::from_str(&stdout_outline).unwrap();
    assert_eq!(json_outline["ok"], false);
    assert_eq!(json_outline["error"]["code"], "OUTLINE_NOT_FOUND");
    assert!(json_outline["error"]["message"]
        .as_str()
        .unwrap()
        .contains("non-existent-outline-id"));
    assert_eq!(
        json_outline["error"]["hint"],
        "run 'aiwc conversation list'"
    );

    // 2. Card not found in JSON
    let mut cmd_card = Command::cargo_bin("aiwc").unwrap();
    let assert_card = cmd_card
        .args([
            "conversation",
            "card",
            "get",
            "non-existent-card-id",
            "--format",
            "json",
        ])
        .assert()
        .failure();
    let stdout_card = String::from_utf8(assert_card.get_output().stdout.clone()).unwrap();
    let json_card: Value = serde_json::from_str(&stdout_card).unwrap();
    assert_eq!(json_card["ok"], false);
    assert_eq!(json_card["error"]["code"], "CARD_NOT_FOUND");
    assert!(json_card["error"]["message"]
        .as_str()
        .unwrap()
        .contains("non-existent-card-id"));
    assert_eq!(
        json_card["error"]["hint"],
        "run 'aiwc conversation outline' or 'aiwc conversation search'"
    );

    // 3. Adapter not found in JSON
    let mut cmd_adapter = Command::cargo_bin("aiwc").unwrap();
    let assert_adapter = cmd_adapter
        .args([
            "conversation",
            "adapter",
            "inspect",
            "non-existent-adapter-id",
            "--format",
            "json",
        ])
        .assert()
        .failure();
    let stdout_adapter = String::from_utf8(assert_adapter.get_output().stdout.clone()).unwrap();
    let json_adapter: Value = serde_json::from_str(&stdout_adapter).unwrap();
    assert_eq!(json_adapter["ok"], false);
    assert_eq!(json_adapter["error"]["code"], "ADAPTER_NOT_FOUND");
    assert!(json_adapter["error"]["message"]
        .as_str()
        .unwrap()
        .contains("non-existent-adapter-id"));
    assert_eq!(
        json_adapter["error"]["hint"],
        "run 'aiwc conversation adapter list'"
    );

    // 4. Search missing query in JSON
    let mut cmd_search = Command::cargo_bin("aiwc").unwrap();
    let assert_search = cmd_search
        .args(["conversation", "search", "--format", "json"])
        .assert()
        .failure();
    let stdout_search = String::from_utf8(assert_search.get_output().stdout.clone()).unwrap();
    let json_search: Value = serde_json::from_str(&stdout_search).unwrap();
    assert_eq!(json_search["ok"], false);
    assert_eq!(json_search["error"]["code"], "INVALID_PARAMETER");
    assert_eq!(
        json_search["error"]["message"],
        "Conversation search query is required"
    );
    assert_eq!(
        json_search["error"]["hint"],
        "run 'aiwc conversation search --query <QUERY>' or 'aiwc conversation list'"
    );
}

#[test]
fn test_conversation_sync_help_options() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd
        .args(["conversation", "sync", "--help"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(output.contains("--full"));
    assert!(output.contains("--incremental"));
    assert!(output.contains("--dry-run"));
    assert!(output.contains("--kind"));
}

#[test]
fn test_conversation_sync_full_and_incremental_conflict() {
    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd
        .args(["conversation", "sync", "--full", "--incremental"])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("cannot be used with") || stderr.contains("conflict"),
        "Expected conflict error message, got: {stderr}"
    );

    // Also verify alias --force conflicts with --incremental
    let mut cmd_force = Command::cargo_bin("aiwc").unwrap();
    let assert_force = cmd_force
        .args(["conversation", "sync", "--force", "--incremental"])
        .assert()
        .failure();
    let stderr_force = String::from_utf8(assert_force.get_output().stderr.clone()).unwrap();
    assert!(
        stderr_force.contains("cannot be used with") || stderr_force.contains("conflict"),
        "Expected conflict error message for --force, got: {stderr_force}"
    );
}

#[test]
fn test_conversation_sync_dry_run_zero_side_effect() {
    let config = asset_core::infrastructure::runtime::RuntimeConfig::from_environment()
        .expect("discover runtime config");
    let conn = rusqlite::Connection::open(&config.db_path).expect("open database");

    let count_rows = |table: &str| -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap_or(0)
    };

    let sessions_before = count_rows("conversation_sessions");
    let turns_before = count_rows("conversation_turns");
    let parts_before = count_rows("conversation_parts");
    let sync_runs_before = count_rows("conversation_sync_runs");

    let mut cmd = Command::cargo_bin("aiwc").unwrap();
    let assert = cmd
        .args(["conversation", "sync", "--dry-run", "--format", "json"])
        .assert()
        .success();

    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).expect("valid json output");
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["dry_run"], true);

    let sessions_after = count_rows("conversation_sessions");
    let turns_after = count_rows("conversation_turns");
    let parts_after = count_rows("conversation_parts");
    let sync_runs_after = count_rows("conversation_sync_runs");

    assert_eq!(
        sessions_before, sessions_after,
        "conversation_sessions table was mutated"
    );
    assert_eq!(
        turns_before, turns_after,
        "conversation_turns table was mutated"
    );
    assert_eq!(
        parts_before, parts_after,
        "conversation_parts table was mutated"
    );
    assert_eq!(
        sync_runs_before, sync_runs_after,
        "conversation_sync_runs table was mutated"
    );
}
