use assert_cmd::Command;
use serde_json::Value;

thread_local! {
    // Each Rust test thread gets an isolated app database, while multiple CLI
    // invocations inside one test still share state when the scenario requires it.
    static CLI_TEST_HOME: tempfile::TempDir = tempfile::tempdir()
        .expect("create isolated CLI test home");
}

fn aiwc_command() -> Command {
    let mut command = Command::cargo_bin("aiwc").expect("aiwc binary is built");
    CLI_TEST_HOME.with(|home| {
        command
            .env("HOME", home.path())
            .env("ASSETIWEAVE_DB_PATH", home.path().join("app.db"));
    });
    command
}

#[test]
fn test_version_command() {
    let mut cmd = aiwc_command();
    cmd.arg("version").assert().success();

    let mut cmd_json = aiwc_command();
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
    let mut cmd = aiwc_command();
    cmd.arg("overview").assert().success();

    let mut cmd_json = aiwc_command();
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
    let mut cmd_ov = aiwc_command();
    cmd_ov.arg("ov").assert().success();

    // aiwc sk list -> skill list
    let mut cmd_sk = aiwc_command();
    cmd_sk.args(["sk", "list"]).assert().success();

    // aiwc c adapter list -> conversation adapter list
    let mut cmd_c = aiwc_command();
    cmd_c.args(["c", "adapter", "list"]).assert().success();
}

#[test]
fn test_doctor_command() {
    let mut cmd = aiwc_command();
    let assert = cmd.args(["doctor", "--format", "json"]).assert().success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["data"]["checks"].is_array());
}

#[test]
fn test_settings_show() {
    let mut cmd = aiwc_command();
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
    let mut cmd = aiwc_command();
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
    let mut cmd = aiwc_command();
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
    let mut cmd = aiwc_command();
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
fn test_conversation_adapter_upgrade_developer_dry_run() {
    let mut cmd = aiwc_command();
    let assert = cmd
        .args([
            "conversation",
            "adapter",
            "upgrade",
            "--developer",
            "--dry-run",
            "--format",
            "json",
        ])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["dry_run"], true);
    assert_eq!(json["data"]["developer"], true);

    // Test short alias: c ad up -d --dry-run
    let mut alias_cmd = aiwc_command();
    let alias_assert = alias_cmd
        .args(["c", "ad", "up", "-d", "--dry-run", "--format", "json"])
        .assert()
        .success();
    let alias_output = String::from_utf8(alias_assert.get_output().stdout.clone()).unwrap();
    let alias_json: Value = serde_json::from_str(&alias_output).unwrap();
    assert_eq!(alias_json["ok"], true);
}

#[test]
fn test_skill_list() {
    let mut cmd = aiwc_command();
    cmd.args(["skill", "list"]).assert().success();

    let mut cmd_json = aiwc_command();
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
    let mut cmd = aiwc_command();
    let assert_table = cmd.args(["skill", "builtin", "list"]).assert().success();
    let table_out = String::from_utf8(assert_table.get_output().stdout.clone()).unwrap();
    assert!(table_out.contains("assetiweave-memory"));
    assert!(table_out.contains("assetiweave-conversation-organizer"));

    // 2. JSON format
    let mut cmd_json = aiwc_command();
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
    let mut cmd = aiwc_command();
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
    let mut cmd = aiwc_command();
    let assert_read = cmd
        .args(["skill", "read", "assetiweave-conversation-organizer"])
        .assert()
        .success();
    let stdout = String::from_utf8(assert_read.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("name: assetiweave-conversation-organizer"));
    assert!(stdout.contains("AssetIWeave 对话记录整理"));

    let mut cmd_sk = aiwc_command();
    let assert_sk = cmd_sk
        .args(["sk", "read", "assetiweave-memory"])
        .assert()
        .success();
    let stdout_sk = String::from_utf8(assert_sk.get_output().stdout.clone()).unwrap();
    assert!(stdout_sk.contains("name: assetiweave-memory"));
}

#[test]
fn test_skill_read_invalid_name() {
    let mut cmd = aiwc_command();
    let assert_err = cmd
        .args(["skill", "read", "non-existent-skill-xyz"])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert_err.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("Skill not found: non-existent-skill-xyz"));
    assert!(stderr.contains("Hint: run 'aiwc skill builtin list'"));

    let mut cmd_builtin = aiwc_command();
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
    let mut cmd = aiwc_command();
    cmd.args(["source", "list"]).assert().success();

    let mut cmd_json = aiwc_command();
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
    let mut cmd = aiwc_command();
    cmd.args(["tenant", "list"]).assert().success();

    let mut cmd_json = aiwc_command();
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
    let mut cmd = aiwc_command();
    cmd.args(["profile", "list"]).assert().success();

    let mut cmd_alias = aiwc_command();
    cmd_alias
        .args(["target-profile", "list"])
        .assert()
        .success();

    let mut cmd_json = aiwc_command();
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
    let mut cmd = aiwc_command();
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
    let mut cmd = aiwc_command();
    let assert = cmd
        .args(["memory", "recent", "get", "--format", "json"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
}

#[test]
fn test_memory_recent_get_empty_state_text() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cmd = aiwc_command();
    let assert = cmd
        .env("ASSETIWEAVE_MEMORY_ROOT", tmp.path())
        .args(["memory", "recent", "get"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert_eq!(output.trim(), "暂无记录");
}

#[test]
fn test_memory_project_get() {
    let mut cmd = aiwc_command();
    let assert = cmd
        .args([
            "memory",
            "project",
            "get",
            "--current-project",
            "--format",
            "json",
        ])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
}

#[test]
fn test_memory_project_get_empty_state_text() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cmd = aiwc_command();
    let assert = cmd
        .env("ASSETIWEAVE_MEMORY_ROOT", tmp.path())
        .args(["memory", "project", "get", "--current-project"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert_eq!(output.trim(), "暂无记录");
}

#[test]
fn test_memory_cli_help_removes_dead_commands() {
    let mut cmd = aiwc_command();
    let assert = cmd.args(["memory", "--help"]).assert().success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    // Active subcommands
    assert!(output.contains("recent"));
    assert!(output.contains("project"));
    assert!(output.contains("state"));
    assert!(output.contains("commit"));
    assert!(output.contains("watermark"));
    assert!(output.contains("status"));

    // Removed / dead subcommands
    assert!(!output.contains("rebuild"));
    assert!(!output.contains("task"));
    assert!(!output.contains("recall"));
    assert!(!output.contains("context"));
}

#[test]
fn test_memory_cli_read_path_rendered_markdown() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("default");
    let recent_dir = root.join("recent");
    let projects_dir = root.join("projects");
    std::fs::create_dir_all(&recent_dir).unwrap();
    std::fs::create_dir_all(&projects_dir).unwrap();

    let recent_md = "# Recent Work\n\n### 🍂 待探索的悬空叶子节点 (Unexplored Branches)\n- [s1#T1](aiwc://session/s1/turn/1): 探索新特性\n\n### ✅ 已完成的闭环任务 (Completed Tasks)\n- [s1#T2](aiwc://session/s1/turn/2): 完成架构升级\n";
    std::fs::write(recent_dir.join("latest.md"), recent_md).unwrap();

    let project_md = "# 项目记忆: /test-proj\n\n## 🍂 待探索的悬空叶子节点 (Unexplored Branches)\n- [s1#T1](aiwc://session/s1/turn/1): 探索新特性\n";
    std::fs::write(projects_dir.join("test-proj.md"), project_md).unwrap();

    let memory_md = "# 全局长期记忆与工程准则 (Global Memory)\n\n## 📐 通用工程规范与架构准则\n- 本地优先与 Markdown 优先\n";
    std::fs::write(root.join("MEMORY.md"), memory_md).unwrap();

    // 1. aiwc memory recent get (text)
    let mut cmd = aiwc_command();
    let assert = cmd
        .env("ASSETIWEAVE_MEMORY_ROOT", tmp.path())
        .args(["memory", "recent", "get"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(output.contains("Recent Work"));
    assert!(output.contains("[s1#T1](aiwc://session/s1/turn/1)"));

    // 2. aiwc memory project get <project_path>
    let mut cmd = aiwc_command();
    let assert = cmd
        .env("ASSETIWEAVE_MEMORY_ROOT", tmp.path())
        .args(["memory", "project", "get", "/test-proj"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(output.contains("项目记忆: /test-proj"));
    assert!(output.contains("[s1#T1](aiwc://session/s1/turn/1)"));
}

#[test]
fn test_memory_cli_state_commit_and_watermark() {
    let tmp = tempfile::tempdir().unwrap();
    let memory_root = tmp.path().join("memories");
    std::fs::create_dir_all(&memory_root).unwrap();

    // 1. Watermark check before any activity
    let mut cmd = aiwc_command();
    let assert = cmd
        .env("ASSETIWEAVE_MEMORY_ROOT", &memory_root)
        .args([
            "memory",
            "watermark",
            "check",
            "--project",
            "test-slug",
            "--session-id",
            "sess-123",
            "--format",
            "json",
        ])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["has_watermark"], false);

    // 2. Advance watermark
    let mut cmd = aiwc_command();
    let assert = cmd
        .env("ASSETIWEAVE_MEMORY_ROOT", &memory_root)
        .args([
            "memory",
            "watermark",
            "advance",
            "--project",
            "test-slug",
            "--session-id",
            "sess-123",
            "--turn",
            "5",
            "--format",
            "json",
        ])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["advanced_to_turn"], 5);

    // 3. Watermark check after advance
    let mut cmd = aiwc_command();
    let assert = cmd
        .env("ASSETIWEAVE_MEMORY_ROOT", &memory_root)
        .args([
            "memory",
            "watermark",
            "check",
            "--project",
            "test-slug",
            "--session-id",
            "sess-123",
            "--format",
            "json",
        ])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["has_watermark"], true);
    assert_eq!(json["data"]["watermark"], 5);

    // 4. Commit evolution
    let mut cmd = aiwc_command();
    let assert = cmd
        .env("ASSETIWEAVE_MEMORY_ROOT", &memory_root)
        .args([
            "memory",
            "commit",
            "--project",
            "test-slug",
            "--topic",
            "tree-arch",
            "--topic-name",
            "Tree Architecture",
            "--summary",
            "State tree migration completed",
            "--session-id",
            "sess-123",
            "--short-id",
            "s123",
            "--turn",
            "6",
            "--format",
            "json",
        ])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["committed_node_id"], "s123#T6");

    // 5. Get project state
    let mut cmd = aiwc_command();
    let assert = cmd
        .env("ASSETIWEAVE_MEMORY_ROOT", &memory_root)
        .args([
            "memory",
            "state",
            "get",
            "--project",
            "test-slug",
            "--format",
            "json",
        ])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["project_slug"], "test-slug");
    assert_eq!(json["data"]["session_watermarks"]["sess-123"], 6);

    // 6. Get recent state
    let mut cmd = aiwc_command();
    let assert = cmd
        .env("ASSETIWEAVE_MEMORY_ROOT", &memory_root)
        .args(["memory", "state", "get", "--format", "json"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["data"]["projects"]["test-slug"].is_object());

    // 7. Check projected markdown through project get
    let mut cmd = aiwc_command();
    let assert = cmd
        .env("ASSETIWEAVE_MEMORY_ROOT", &memory_root)
        .args(["memory", "project", "get", "test-slug"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(output.contains("# 项目记忆: test-slug"));
    assert!(output.contains("[s123#T6](aiwc://session/s123/turn/6): "));
    assert!(output.contains("State tree migration completed"));

    // 8. Test memory state get --raw (un-enveloped)
    let mut cmd = aiwc_command();
    let assert = cmd
        .env("ASSETIWEAVE_MEMORY_ROOT", &memory_root)
        .args(["memory", "state", "get", "--project", "test-slug", "--raw"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let raw_json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(raw_json["project_slug"], "test-slug");
    assert!(raw_json.get("ok").is_none());

    // 9. Test memory commit --file <path>
    let commit_file = tmp.path().join("commit.json");
    let payload_json = serde_json::json!({
        "project_slug": "test-slug",
        "topic_id": "tree-arch",
        "topic_name": "Tree Architecture",
        "parent_node_id": "s123#T6",
        "parent_new_status": "continued",
        "new_node": {
            "id": "s123#T7",
            "parent_id": "s123#T6",
            "topic_id": "tree-arch",
            "summary": "Continued via --file payload",
            "status": "completed",
            "source_session_id": "sess-123",
            "turn_index": 7,
            "created_at": "2026-11-02T12:00:00Z",
            "updated_at": "2026-11-02T12:00:00Z"
        },
        "unexplored_siblings": []
    });
    std::fs::write(&commit_file, serde_json::to_string(&payload_json).unwrap()).unwrap();

    let mut cmd = aiwc_command();
    let assert = cmd
        .env("ASSETIWEAVE_MEMORY_ROOT", &memory_root)
        .args([
            "memory",
            "commit",
            "--file",
            commit_file.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["committed_node_id"], "s123#T7");
}

#[test]
fn test_memory_cli_minimal_commit_and_batch() {
    let tmp = tempfile::tempdir().unwrap();
    let memory_root = tmp.path().join("memories");
    std::fs::create_dir_all(&memory_root).unwrap();

    // 1. Commit evolution with simplified agent schema and unexplored siblings
    let mut cmd = aiwc_command();
    let assert = cmd
        .env("ASSETIWEAVE_MEMORY_ROOT", &memory_root)
        .args([
            "memory",
            "commit",
            "--project",
            "my-app",
            "--summary",
            "Initial setup completed",
            "--session-id",
            "sess-9999",
            "--short-id",
            "s9999",
            "--turn",
            "1",
            "--unexplored",
            "Explore database caching",
            "--unexplored",
            "Explore background workers",
            "--format",
            "json",
        ])
        .assert()
        .success();

    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["committed_node_id"], "s9999#T1");
    let unexp = json["data"]["unexplored_node_ids"].as_array().unwrap();
    assert_eq!(unexp.len(), 2);
    assert_eq!(unexp[0], "s9999#T1.1");
    assert_eq!(unexp[1], "s9999#T1.2");

    // 2. Batch commit with simplified JSON: evolve child, advance watermark
    let batch_file = tmp.path().join("batch.json");
    let batch_json = serde_json::json!({
        "window": "24h",
        "commits": [
            {
                "project": "my-app",
                "session_id": "sess-9999",
                "short_id": "s9999",
                "turn": 2,
                "summary": "Implemented database caching",
                "parent_id": "s9999#T1.1",
                "parent_new_status": "completed",
                "status": "completed"
            }
        ],
        "advance_watermarks": [
            {
                "project": "my-app",
                "session_id": "sess-9999",
                "turn": 2
            }
        ]
    });
    std::fs::write(&batch_file, serde_json::to_string(&batch_json).unwrap()).unwrap();

    let mut cmd = aiwc_command();
    let assert = cmd
        .env("ASSETIWEAVE_MEMORY_ROOT", &memory_root)
        .args([
            "memory",
            "commit",
            "--file",
            batch_file.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success();

    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["data"]["commits"].is_array());
    assert_eq!(json["data"]["commits"][0]["committed_node_id"], "s9999#T2");

    // 3. Check projects/my-app.md markdown projection
    let proj_md_path = memory_root
        .join("default")
        .join("projects")
        .join("my-app.md");
    let proj_md = std::fs::read_to_string(&proj_md_path).unwrap();
    // Must contain timestamp and ancestry
    assert!(proj_md.contains("- [s9999#T2](aiwc://session/s9999/turn/2): ["));
    assert!(proj_md.contains("Implemented database caching *(演进自 [s9999#T1.1])*"));

    // 4. Check recent/latest.md projection
    let recent_md_path = memory_root.join("default").join("recent").join("latest.md");
    let recent_md = std::fs::read_to_string(&recent_md_path).unwrap();
    assert!(recent_md.contains("## 项目: my-app"));
    assert!(recent_md.contains("- [s9999#T2](aiwc://session/s9999/turn/2): ["));
    assert!(recent_md.contains("Implemented database caching *(演进自 [s9999#T1.1])*"));
}

#[test]
fn test_memory_cli_status_dismissed_tombstone() {
    let tmp = tempfile::tempdir().unwrap();
    let memory_root = tmp.path().join("memories");
    std::fs::create_dir_all(&memory_root).unwrap();

    // 1. Commit initial turn with unexplored sibling s88#T1.1
    let mut cmd = aiwc_command();
    let assert = cmd
        .env("ASSETIWEAVE_MEMORY_ROOT", &memory_root)
        .args([
            "memory",
            "commit",
            "--project",
            "tombstone-proj",
            "--summary",
            "Root feature explored",
            "--session-id",
            "sess-8888",
            "--short-id",
            "s88",
            "--turn",
            "1",
            "--unexplored",
            "Flawed idea to dismiss",
            "--format",
            "json",
        ])
        .assert()
        .success();

    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["unexplored_node_ids"][0], "s88#T1.1");

    // Verify s88#T1.1 is in project markdown and recent markdown
    let proj_md_path = memory_root
        .join("default")
        .join("projects")
        .join("tombstone-proj.md");
    let proj_md = std::fs::read_to_string(&proj_md_path).unwrap();
    assert!(proj_md.contains("s88#T1.1"));

    let recent_md_path = memory_root.join("default").join("recent").join("latest.md");
    let recent_md = std::fs::read_to_string(&recent_md_path).unwrap();
    assert!(recent_md.contains("s88#T1.1"));

    // 2. Change status of s88#T1.1 to dismissed
    let mut cmd = aiwc_command();
    let assert = cmd
        .env("ASSETIWEAVE_MEMORY_ROOT", &memory_root)
        .args([
            "memory",
            "status",
            "s88#T1.1",
            "dismissed",
            "--project",
            "tombstone-proj",
            "--format",
            "json",
        ])
        .assert()
        .success();

    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["status"], "dismissed");

    // 3. Check .state/projects/tombstone-proj.json: node has status dismissed
    let state_file = memory_root
        .join("default")
        .join(".state")
        .join("projects")
        .join("tombstone-proj.json");
    let state_str = std::fs::read_to_string(&state_file).unwrap();
    let state_val: Value = serde_json::from_str(&state_str).unwrap();
    let node_status = state_val["topic_trees"]["general"]["nodes"]["s88#T1.1"]["status"]
        .as_str()
        .unwrap();
    assert_eq!(node_status, "dismissed");

    // 4. Node disappears from projects/tombstone-proj.md and recent/latest.md
    let proj_md = std::fs::read_to_string(&proj_md_path).unwrap();
    assert!(!proj_md.contains("s88#T1.1"));
    let recent_md = std::fs::read_to_string(&recent_md_path).unwrap();
    assert!(!recent_md.contains("s88#T1.1"));

    // 5. Re-committing with unexplored sibling with identical ID does NOT resurrect it
    let resurrect_payload = serde_json::json!({
        "project": "tombstone-proj",
        "session_id": "sess-8888",
        "short_id": "s88",
        "turn": 2,
        "summary": "Turn 2 attempt resurrection",
        "unexplored": [
            {
                "id": "s88#T1.1",
                "summary": "Resurrection attempt"
            }
        ]
    });
    let resurrect_file = tmp.path().join("resurrect.json");
    std::fs::write(
        &resurrect_file,
        serde_json::to_string(&resurrect_payload).unwrap(),
    )
    .unwrap();

    let mut cmd = aiwc_command();
    let assert = cmd
        .env("ASSETIWEAVE_MEMORY_ROOT", &memory_root)
        .args([
            "memory",
            "commit",
            "--file",
            resurrect_file.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);

    // Verify node is STILL dismissed in .state and NOT in markdown!
    let state_str = std::fs::read_to_string(&state_file).unwrap();
    let state_val: Value = serde_json::from_str(&state_str).unwrap();
    let node_status = state_val["topic_trees"]["general"]["nodes"]["s88#T1.1"]["status"]
        .as_str()
        .unwrap();
    assert_eq!(node_status, "dismissed");

    let proj_md = std::fs::read_to_string(&proj_md_path).unwrap();
    assert!(!proj_md.contains("s88#T1.1"));
}

#[test]
fn test_completion() {
    let mut cmd = aiwc_command();
    let assert = cmd.args(["completion", "bash"]).assert().success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(output.contains("_aiwc"));
}

#[test]
fn test_conversation_outline_help() {
    let mut cmd = aiwc_command();
    let assert = cmd
        .args(["conversation", "outline", "--help"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(output.contains("outline"));
}

#[test]
fn test_conversation_list_command() {
    let mut cmd = aiwc_command();
    cmd.args(["conversation", "list", "--limit", "5"])
        .assert()
        .success();

    let mut cmd_json = aiwc_command();
    let assert = cmd_json
        .args(["conversation", "list", "--limit", "3", "--format", "json"])
        .assert()
        .success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert!(json["data"].is_array());

    let mut cmd_window = aiwc_command();
    cmd_window
        .args(["conversation", "list", "--window", "24h", "--limit", "5"])
        .assert()
        .success();

    let mut cmd_range = aiwc_command();
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

    let mut cmd_since_dt = aiwc_command();
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
    let mut cmd_time_hours = aiwc_command();
    cmd_time_hours
        .args(["conversation", "list", "--time", "24", "--limit", "5"])
        .assert()
        .success();

    // 2. Single datetime (A to now)
    let mut cmd_time_since = aiwc_command();
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
    let mut cmd_time_two = aiwc_command();
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
    let mut cmd_time_json = aiwc_command();
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
    let mut cmd_outline = aiwc_command();
    let assert_outline = cmd_outline
        .args(["conversation", "outline", "--help"])
        .assert()
        .success();
    let out_outline = String::from_utf8(assert_outline.get_output().stdout.clone()).unwrap();
    assert!(out_outline.contains("outline"));

    let mut cmd_card = aiwc_command();
    let assert_card = cmd_card
        .args(["conversation", "card", "--help"])
        .assert()
        .success();
    let out_card = String::from_utf8(assert_card.get_output().stdout.clone()).unwrap();
    assert!(out_card.contains("get"));

    let mut cmd_card_get = aiwc_command();
    let assert_card_get = cmd_card_get
        .args(["conversation", "card", "get", "--help"])
        .assert()
        .success();
    let out_card_get = String::from_utf8(assert_card_get.get_output().stdout.clone()).unwrap();
    assert!(out_card_get.contains("--raw"));
}

#[test]
fn test_conversation_outline_and_card_drilldown() {
    let mut cmd_list = aiwc_command();
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
    let mut cmd_so = aiwc_command();
    let assert_so = cmd_so
        .args(["conversation", "outline", session_id])
        .assert()
        .success();
    let out_so = String::from_utf8(assert_so.get_output().stdout.clone()).unwrap();
    let json_so: Value = serde_json::from_str(&out_so).unwrap();
    assert_eq!(json_so["ok"], true);
    assert_eq!(json_so["data"]["level"], "session");
    assert_eq!(
        json_so["data"]["turns"]["columns"],
        serde_json::json!([
            "index",
            "turn_id",
            "question",
            "activities",
            "cards",
            "started_at"
        ])
    );
    assert!(json_so["data"]["turns"]["rows"].is_array());

    // 1.1 Test --questions mode
    let mut cmd_q = aiwc_command();
    let assert_q = cmd_q
        .args(["conversation", "outline", session_id, "--questions"])
        .assert()
        .success();
    let out_q = String::from_utf8(assert_q.get_output().stdout.clone()).unwrap();
    let json_q: Value = serde_json::from_str(&out_q).unwrap();
    assert_eq!(json_q["ok"], true);
    assert_eq!(
        json_q["data"]["turns"]["columns"],
        serde_json::json!(["index", "turn_id", "question", "started_at"])
    );

    // 1.2 Test --qa mode
    let mut cmd_qa = aiwc_command();
    let assert_qa = cmd_qa
        .args(["conversation", "outline", session_id, "--qa"])
        .assert()
        .success();
    let out_qa = String::from_utf8(assert_qa.get_output().stdout.clone()).unwrap();
    let json_qa: Value = serde_json::from_str(&out_qa).unwrap();
    assert_eq!(json_qa["ok"], true);
    assert_eq!(
        json_qa["data"]["turns"]["columns"],
        serde_json::json!(["index", "turn_id", "question", "answer", "started_at"])
    );

    // 1.3 Test --full mode
    let mut cmd_full = aiwc_command();
    let assert_full = cmd_full
        .args(["conversation", "outline", session_id, "--full"])
        .assert()
        .success();
    let out_full = String::from_utf8(assert_full.get_output().stdout.clone()).unwrap();
    let json_full: Value = serde_json::from_str(&out_full).unwrap();
    assert_eq!(json_full["ok"], true);
    assert_eq!(
        json_full["data"]["turns"]["columns"],
        serde_json::json!([
            "index",
            "turn_id",
            "question",
            "activities",
            "cards",
            "answer",
            "started_at"
        ])
    );

    // 1.4 Test mutually exclusive flags conflict
    let mut cmd_conflict = aiwc_command();
    cmd_conflict
        .args(["conversation", "outline", session_id, "--questions", "--qa"])
        .assert()
        .failure();

    // If turns exist in the session, drill down to turn outline
    let rows = json_so["data"]["turns"]["rows"].as_array().unwrap();
    if let Some(first_row) = rows.first() {
        let turn_id = first_row[1].as_str().unwrap();

        // 3. Turn outline via `conversation outline <turn_id>`
        let mut cmd_to = aiwc_command();
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
            let mut cmd_co = aiwc_command();
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
            let mut cmd_cg = aiwc_command();
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
            let mut cmd_cgr = aiwc_command();
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
fn test_conversation_outline_exposes_subagent_semantics() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("subagent_outline.db");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let pool = asset_core::store::open_migrated_pool(&db_path)
            .await
            .expect("open migrated pool");
        let tenant_id = "default";
        let now = "2026-10-27T00:00:00Z".to_string();

        let adapter = asset_core::domain::ConversationAdapter {
            id: "codex".to_string(),
            name: "Codex Adapter".to_string(),
            kind: asset_core::domain::ConversationAdapterKind::External,
            version: "0.1.0".to_string(),
            enabled: true,
            manifest_path: None,
            executable_path: None,
            content_hash: None,
            trusted_hash: None,
            trust_state: asset_core::domain::ConversationAdapterTrustState::Trusted,
            protocol_version: Some(1),
            capabilities: vec!["read_session".to_string()],
            input_kinds: vec![asset_core::domain::ConversationSourceKind::Directory],
            card_contract_version: Some(1),
            card_kinds: Vec::new(),
            projection_version: Some(1),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        let source = asset_core::domain::ConversationSource {
            id: "codex-source".to_string(),
            adapter_id: "codex".to_string(),
            name: "Codex Source".to_string(),
            kind: asset_core::domain::ConversationSourceKind::Directory,
            location: temp_dir.path().to_string_lossy().to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        };

        // ADR-0022：一次人类提问 = 一个父 Turn，子代理是独立且不可见的会话，
        // 父 Turn 里只留一枚 `kind = subagent` 的委派锚点。
        let parent = asset_core::domain::NormalizedConversationSession {
            external_id: "session-subagent-outline".to_string(),
            title: Some("子代理大纲".to_string()),
            started_at: Some(now.clone()),
            updated_at: Some(now.clone()),
            turns: vec![asset_core::domain::NormalizedConversationTurn {
                external_id: Some("turn-ext-main".to_string()),
                turn_index: 0,
                user_text: "Plan the refactor".to_string(),
                parts: vec![
                    asset_core::domain::NormalizedConversationPart {
                        role: asset_core::domain::ConversationPartRole::Assistant,
                        kind: asset_core::domain::ConversationPartKind::Text,
                        text: Some("Plan ready.".to_string()),
                        language: None,
                        command: None,
                        cwd: None,
                        status: None,
                        exit_code: None,
                        command_label: None,
                        source_execution_id: None,
                        content_card: None,
                        metadata_json: None,
                    },
                    asset_core::domain::NormalizedConversationPart {
                        role: asset_core::domain::ConversationPartRole::Assistant,
                        kind: asset_core::domain::ConversationPartKind::Command,
                        text: None,
                        language: None,
                        command: Some("cargo test -p cli".to_string()),
                        cwd: None,
                        status: Some("success".to_string()),
                        exit_code: Some(0),
                        command_label: Some("test".to_string()),
                        source_execution_id: None,
                        content_card: None,
                        metadata_json: None,
                    },
                    asset_core::domain::NormalizedConversationPart {
                        role: asset_core::domain::ConversationPartRole::Assistant,
                        kind: asset_core::domain::ConversationPartKind::Subagent,
                        text: Some("[委派任务] Review the diff".to_string()),
                        language: None,
                        command: None,
                        cwd: None,
                        status: Some("completed".to_string()),
                        exit_code: None,
                        command_label: None,
                        source_execution_id: None,
                        content_card: None,
                        metadata_json: Some(
                            serde_json::json!({
                                "child_external_id": "session-subagent-reviewer",
                                "agent_role": "Rust Core Reviewer 2",
                                "title": "Review the diff",
                                "report_summary": "发现两处越权写库",
                            })
                            .to_string(),
                        ),
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };

        let child = asset_core::domain::NormalizedConversationSession {
            external_id: "session-subagent-reviewer".to_string(),
            title: Some("Rust Core Reviewer 2".to_string()),
            started_at: Some(now.clone()),
            updated_at: Some(now.clone()),
            execution_origin: Some("subagent".to_string()),
            execution_purpose: Some(
                serde_json::json!({
                    "parent_external_id": "session-subagent-outline",
                    "agent_role": "Rust Core Reviewer 2",
                    "parent_turn_index": 0,
                })
                .to_string(),
            ),
            turns: vec![asset_core::domain::NormalizedConversationTurn {
                external_id: Some("turn-ext-reviewer".to_string()),
                turn_index: 0,
                user_text: "Review the diff".to_string(),
                parts: vec![
                    asset_core::domain::NormalizedConversationPart {
                        role: asset_core::domain::ConversationPartRole::Tool,
                        kind: asset_core::domain::ConversationPartKind::Tool,
                        text: Some("子代理内部独有词 CLI_SUBAGENT_TOKEN_4471".to_string()),
                        language: None,
                        command: None,
                        cwd: None,
                        status: Some("failure".to_string()),
                        exit_code: Some(1),
                        command_label: None,
                        source_execution_id: None,
                        content_card: None,
                        metadata_json: None,
                    },
                    asset_core::domain::NormalizedConversationPart {
                        role: asset_core::domain::ConversationPartRole::Assistant,
                        kind: asset_core::domain::ConversationPartKind::Text,
                        text: Some(
                            "审阅结论：发现两处越权写库 CLI_SUBAGENT_TOKEN_4471".to_string(),
                        ),
                        language: None,
                        command: None,
                        cwd: None,
                        status: None,
                        exit_code: None,
                        command_label: None,
                        source_execution_id: None,
                        content_card: None,
                        metadata_json: None,
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };

        asset_core::store::upsert_conversation_adapter_sqlx(&pool, tenant_id, &adapter)
            .await
            .expect("upsert adapter");
        asset_core::store::upsert_conversation_source_sqlx(&pool, tenant_id, &source)
            .await
            .expect("upsert source");
        asset_core::store::import_conversation_sessions_sqlx(
            &pool,
            tenant_id,
            &source,
            &[parent, child],
            false,
        )
        .await
        .expect("import parent and child sessions");
    });

    // 会话短 ID 通过 CLI 自身拿取，避免测试直接依赖落库结构。
    let mut cmd_list = aiwc_command();
    let assert_list = cmd_list
        .env("ASSETIWEAVE_DB_PATH", &db_path)
        .args(["conversation", "list", "--limit", "5", "--format", "json"])
        .assert()
        .success();
    let json_list: Value =
        serde_json::from_slice(&assert_list.get_output().stdout).expect("valid list json");
    let listed = json_list["data"].as_array().expect("list data");
    assert_eq!(listed.len(), 1, "Subagent Session 默认不可见");
    let session_short_id = listed[0]["short_id"]
        .as_str()
        .expect("seeded parent session must appear in list")
        .to_string();

    // 1. `--parent`：按父会话枚举隐藏的子代理会话。
    let mut cmd_children = aiwc_command();
    let assert_children = cmd_children
        .env("ASSETIWEAVE_DB_PATH", &db_path)
        .args([
            "conversation",
            "list",
            "--parent",
            &session_short_id,
            "--format",
            "json",
        ])
        .assert()
        .success();
    let json_children: Value =
        serde_json::from_slice(&assert_children.get_output().stdout).expect("valid children json");
    let children = json_children["data"].as_array().expect("children data");
    assert_eq!(children.len(), 1, "只应列出该父会话名下的子代理会话");
    let child_short_id = children[0]["short_id"]
        .as_str()
        .expect("child short id")
        .to_string();

    // 2. 父会话大纲：伪轮次列已物理删除，子代理通过 activities 摘要下钻。
    let mut cmd = aiwc_command();
    let assert = cmd
        .env("ASSETIWEAVE_DB_PATH", &db_path)
        .args(["conversation", "outline", &session_short_id])
        .assert()
        .success();
    let raw_outline = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&raw_outline).expect("valid json");
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["level"], "session");
    assert_eq!(
        json["data"]["turns"]["columns"],
        serde_json::json!([
            "index",
            "turn_id",
            "question",
            "activities",
            "cards",
            "started_at"
        ])
    );
    for forbidden in ["turn_kind", "agent_role", "parent_turn_id"] {
        assert!(
            !raw_outline.contains(forbidden),
            "{forbidden} must not survive in the outline contract"
        );
    }

    let rows = json["data"]["turns"]["rows"]
        .as_array()
        .expect("rows is array");
    assert_eq!(
        rows.len(),
        1,
        "1 次人类提问 = 1 个 Turn，子代理不再并列成伪轮次"
    );
    let parent_row = &rows[0];
    assert_eq!(parent_row[2], "Plan the refactor");
    assert_eq!(parent_row[4], 3);
    let activities = parent_row[3].as_array().expect("activities array");
    assert_eq!(activities.first().and_then(Value::as_str), Some("test"));
    assert_eq!(
        activities.last().and_then(Value::as_str),
        Some(format!("Subagent(Rust Core Reviewer 2 -> {child_short_id})").as_str())
    );
    let parent_turn_short_id = parent_row[1].as_str().unwrap().to_string();

    // 3. 子会话大纲：头部透出父会话短 ID 与角色面包屑。
    let mut cmd_child_outline = aiwc_command();
    let assert_child_outline = cmd_child_outline
        .env("ASSETIWEAVE_DB_PATH", &db_path)
        .args(["conversation", "outline", &child_short_id])
        .assert()
        .success();
    let json_child: Value = serde_json::from_slice(&assert_child_outline.get_output().stdout)
        .expect("valid child json");
    assert_eq!(json_child["data"]["level"], "session");
    assert_eq!(json_child["data"]["session_id"], child_short_id);
    assert_eq!(json_child["data"]["parent_session_id"], session_short_id);
    assert_eq!(json_child["data"]["agent_role"], "Rust Core Reviewer 2");

    // 4. 列表全文检索：只出现在子会话内部的词，必须把可见父会话顶上来。
    let mut cmd_query = aiwc_command();
    let assert_query = cmd_query
        .env("ASSETIWEAVE_DB_PATH", &db_path)
        .args([
            "conversation",
            "list",
            "--query",
            "CLI_SUBAGENT_TOKEN_4471",
            "--format",
            "json",
        ])
        .assert()
        .success();
    let json_query: Value =
        serde_json::from_slice(&assert_query.get_output().stdout).expect("valid query json");
    let matched = json_query["data"].as_array().expect("query data");
    assert_eq!(matched.len(), 1, "只能返回可见的父会话");
    assert_eq!(matched[0]["short_id"], session_short_id);

    // 5. Turn 级大纲：委派锚点卡片携带 8 位 `child_session_id`。
    let mut cmd_turn = aiwc_command();
    let assert_turn = cmd_turn
        .env("ASSETIWEAVE_DB_PATH", &db_path)
        .args(["conversation", "outline", &parent_turn_short_id])
        .assert()
        .success();
    let json_turn: Value =
        serde_json::from_slice(&assert_turn.get_output().stdout).expect("valid turn json");
    assert_eq!(json_turn["data"]["level"], "turn");
    assert_eq!(json_turn["data"]["turn_id"], parent_turn_short_id);
    assert_eq!(
        json_turn["data"]["cards"]["columns"],
        serde_json::json!([
            "order",
            "card_id",
            "kind",
            "label",
            "status",
            "chars",
            "child_session_id"
        ])
    );
    let anchor_row = json_turn["data"]["cards"]["rows"]
        .as_array()
        .expect("cards rows")
        .iter()
        .find(|row| {
            row[2]
                .as_str()
                .is_some_and(|kind| kind.ends_with("subagent"))
        })
        .expect("subagent dispatch anchor card");
    assert_eq!(anchor_row[3], "Review the diff");
    assert_eq!(anchor_row[6], child_short_id);

    // 6. 引用定位：锚点卡片必须落回父 Turn。
    let card_id = anchor_row[1].as_str().expect("anchor card id").to_string();
    let mut cmd_card = aiwc_command();
    let assert_card = cmd_card
        .env("ASSETIWEAVE_DB_PATH", &db_path)
        .args(["conversation", "outline", &card_id])
        .assert()
        .success();
    let json_card: Value =
        serde_json::from_slice(&assert_card.get_output().stdout).expect("valid card json");
    assert_eq!(json_card["data"]["level"], "card");
    assert_eq!(json_card["data"]["turn_id"], parent_turn_short_id);

    // 7. 卡片检索：命中子会话内部时附带父会话溯源。
    let mut cmd_search = aiwc_command();
    let assert_search = cmd_search
        .env("ASSETIWEAVE_DB_PATH", &db_path)
        .args([
            "conversation",
            "search",
            "--query",
            "CLI_SUBAGENT_TOKEN_4471",
            "--format",
            "json",
        ])
        .assert()
        .success();
    let json_search: Value =
        serde_json::from_slice(&assert_search.get_output().stdout).expect("valid search json");
    let hits = json_search["data"]["hits"].as_array().expect("search hits");
    assert!(!hits.is_empty(), "子会话内部必须可被检索命中");
    assert!(hits.iter().any(|hit| {
        hit["parent_session_id"]
            .as_str()
            .is_some_and(|id| asset_core::domain::conversation_id_fragment(id) == session_short_id)
            && hit["parent_turn_index"].as_i64() == Some(0)
    }));
}

#[test]
fn test_conversation_affordance_long_help_and_concise_short_help() {
    // 1. Top-level short help `conversation -h` should be concise and contain no affordance
    let mut cmd_short = aiwc_command();
    let assert_short = cmd_short.args(["conversation", "-h"]).assert().success();
    let out_short = String::from_utf8(assert_short.get_output().stdout.clone()).unwrap();
    assert!(out_short.contains("Usage:") && out_short.contains("conversation"));
    assert!(!out_short.contains("Affordance:"));
    assert!(!out_short.contains("Use this when"));
    assert!(!out_short.contains("Avoid when"));

    // 2. Top-level long help `conversation --help` should contain 5-dimension affordance
    let mut cmd_long = aiwc_command();
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
    let mut cmd_search_short = aiwc_command();
    let assert_search_short = cmd_search_short
        .args(["conversation", "search", "-h"])
        .assert()
        .success();
    let out_search_short =
        String::from_utf8(assert_search_short.get_output().stdout.clone()).unwrap();
    assert!(
        out_search_short.contains("Usage:") && out_search_short.contains("conversation search")
    );
    assert!(!out_search_short.contains("Affordance:"));
    assert!(!out_search_short.contains("Use this when"));

    // 4. Subcommand long help `conversation search --help` should contain 5-dimension affordance
    let mut cmd_search_long = aiwc_command();
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
        let mut cmd_sub = aiwc_command();
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
    let mut cmd_outline = aiwc_command();
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
    let mut cmd_card = aiwc_command();
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
    let mut cmd_adapter = aiwc_command();
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
    let mut cmd_search = aiwc_command();
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
    let mut cmd_outline = aiwc_command();
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
    let mut cmd_card = aiwc_command();
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
    let mut cmd_adapter = aiwc_command();
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
    let mut cmd_search = aiwc_command();
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
    let mut cmd = aiwc_command();
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
    let mut cmd = aiwc_command();
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
    let mut cmd_force = aiwc_command();
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
fn test_conversation_sync_full_contract() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut cmd = aiwc_command();
    let assert = cmd
        .env("HOME", temp_dir.path())
        .args([
            "conversation",
            "sync",
            "--full",
            "--dry-run",
            "--format",
            "json",
        ])
        .assert()
        .success();

    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).expect("valid json output");
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["mode"], "full");
    assert_eq!(json["data"]["dry_run"], true);
}

#[test]
fn test_conversation_sync_dry_run_zero_side_effect() {
    let temp_dir = tempfile::tempdir().unwrap();

    let mut cmd = aiwc_command();
    let assert = cmd
        .env("HOME", temp_dir.path())
        .args(["conversation", "sync", "--dry-run", "--format", "json"])
        .assert()
        .success();

    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: Value = serde_json::from_str(&output).expect("valid json output");
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["dry_run"], true);

    // Verify zero side effect using blackbox CLI command in the same isolated sandbox
    let mut list_cmd = aiwc_command();
    let list_assert = list_cmd
        .env("HOME", temp_dir.path())
        .args(["conversation", "list", "--format", "json"])
        .assert()
        .success();

    let list_out = String::from_utf8(list_assert.get_output().stdout.clone()).unwrap();
    let list_json: Value = serde_json::from_str(&list_out).expect("valid json output");
    assert_eq!(list_json["ok"], true);
    let sessions = list_json["data"].as_array().expect("sessions array");
    assert!(
        sessions.is_empty(),
        "Expected 0 sessions after dry-run, found {}",
        sessions.len()
    );
}

#[test]
fn test_conversation_sync_pipeline_clean_in_non_tty() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut cmd = aiwc_command();
    let assert = cmd
        .env("HOME", temp_dir.path())
        .args(["conversation", "sync", "--dry-run", "--format", "json"])
        .assert()
        .success();

    let output = assert.get_output();
    let json: Value = serde_json::from_slice(&output.stdout).expect("stdout must be valid json");
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["dry_run"], true);

    let stderr_str = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr_str.contains('\r'),
        "stderr should not contain carriage returns in non-tty mode: {stderr_str}"
    );
    assert!(
        !stderr_str.contains("\x1b["),
        "stderr should not contain ANSI escape codes in non-tty mode: {stderr_str}"
    );
}

#[test]
fn test_conversation_search_command_keyword_vfox() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("vfox_test.db");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let pool = asset_core::store::open_migrated_pool(&db_path)
            .await
            .expect("open migrated pool");
        let tenant_id = "default";
        let now = "2026-10-25T00:00:00Z".to_string();

        let adapter = asset_core::domain::ConversationAdapter {
            id: "codex".to_string(),
            name: "Codex Adapter".to_string(),
            kind: asset_core::domain::ConversationAdapterKind::External,
            version: "0.1.0".to_string(),
            enabled: true,
            manifest_path: None,
            executable_path: None,
            content_hash: None,
            trusted_hash: None,
            trust_state: asset_core::domain::ConversationAdapterTrustState::Trusted,
            protocol_version: Some(1),
            capabilities: vec!["list".to_string()],
            input_kinds: vec![asset_core::domain::ConversationSourceKind::Directory],
            card_contract_version: Some(1),
            card_kinds: Vec::new(),
            projection_version: Some(1),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        let source = asset_core::domain::ConversationSource {
            id: "codex-source".to_string(),
            adapter_id: "codex".to_string(),
            name: "Codex Source".to_string(),
            kind: asset_core::domain::ConversationSourceKind::Directory,
            location: temp_dir.path().to_string_lossy().to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        };

        asset_core::store::upsert_conversation_adapter_sqlx(&pool, tenant_id, &adapter)
            .await
            .expect("upsert adapter");
        asset_core::store::upsert_conversation_source_sqlx(&pool, tenant_id, &source)
            .await
            .expect("upsert source");

        // 构造会话：prompt 与 response 均不含 'vfox'，仅命令包含 'vfox use nodejs@22'
        let session = asset_core::domain::NormalizedConversationSession {
            external_id: "vfox-session-ext".to_string(),
            title: Some("环境准备测试".to_string()),
            project_path: None,
            started_at: None,
            updated_at: None,
            source_locator: None,
            source_fingerprint: None,
            turns: vec![asset_core::domain::NormalizedConversationTurn {
                external_id: Some("turn-vfox".to_string()),
                turn_index: 0,
                user_text: "请帮我检查并准备 Node.js 运行环境".to_string(),
                title: None,
                started_at: None,
                ended_at: None,
                model: None,
                parts: vec![
                    asset_core::domain::NormalizedConversationPart {
                        role: asset_core::domain::ConversationPartRole::Tool,
                        kind: asset_core::domain::ConversationPartKind::Command,
                        text: None,
                        language: None,
                        command: Some("vfox use nodejs@22".to_string()),
                        cwd: Some("/workspace/app".to_string()),
                        status: Some("success".to_string()),
                        exit_code: Some(0),
                        command_label: Some("切换 Node.js 版本".to_string()),
                        source_execution_id: None,
                        content_card: None,
                        metadata_json: None,
                    },
                    asset_core::domain::NormalizedConversationPart {
                        role: asset_core::domain::ConversationPartRole::Assistant,
                        kind: asset_core::domain::ConversationPartKind::Text,
                        text: Some("已为您成功配置运行环境，可以开始开发调试。".to_string()),
                        language: None,
                        command: None,
                        cwd: None,
                        status: None,
                        exit_code: None,
                        command_label: None,
                        source_execution_id: None,
                        content_card: None,
                        metadata_json: None,
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };

        asset_core::store::import_conversation_sessions_sqlx(
            &pool,
            tenant_id,
            &source,
            &[session],
            false,
        )
        .await
        .expect("import session");
    });

    // 1. JSON 格式搜索验证：aiwc conversation search --query vfox --format json
    let mut cmd_json = aiwc_command();
    let assert_json = cmd_json
        .env("ASSETIWEAVE_DB_PATH", &db_path)
        .args([
            "conversation",
            "search",
            "--query",
            "vfox",
            "--format",
            "json",
        ])
        .assert()
        .success();

    let output_json =
        String::from_utf8(assert_json.get_output().stdout.clone()).expect("valid utf8");
    let res_json: Value = serde_json::from_str(&output_json).expect("valid json search result");
    assert_eq!(res_json["ok"], true);
    let hits = res_json["data"]["hits"].as_array().expect("hits is array");
    assert_eq!(
        hits.len(),
        1,
        "应精确命中 1 条包含 vfox 的卡片，实际结果: {output_json}"
    );
    let hit = &hits[0];
    assert!(
        hit["snippet"].as_str().unwrap().contains("vfox"),
        "命中卡片的 snippet 必须包含关键字 'vfox'"
    );

    // 2. 文本格式搜索验证：aiwc conversation search --query vfox
    let mut cmd_text = aiwc_command();
    let assert_text = cmd_text
        .env("ASSETIWEAVE_DB_PATH", &db_path)
        .args(["conversation", "search", "--query", "vfox"])
        .assert()
        .success();

    let output_text =
        String::from_utf8(assert_text.get_output().stdout.clone()).expect("valid utf8");
    assert!(
        output_text.contains("vfox"),
        "文本输出必须包含命中命令关键字 'vfox'"
    );
}

#[test]
fn test_conversation_list_unprocessed_flag_and_watermark_view_model() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("unprocessed_test.db");
    let mem_dir = tempfile::tempdir().unwrap();

    let project_path = "/code/test-unprocessed-proj";
    let project_slug = asset_core::application::memory::project_path_to_slug(project_path);

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let pool = asset_core::store::open_migrated_pool(&db_path)
            .await
            .expect("open migrated pool");
        let tenant_id = "default";
        let now = "2026-11-04T00:00:00Z".to_string();

        let adapter = asset_core::domain::ConversationAdapter {
            id: "test-adapter-unproc".to_string(),
            name: "Test Adapter".to_string(),
            kind: asset_core::domain::ConversationAdapterKind::External,
            version: "0.1.0".to_string(),
            enabled: true,
            manifest_path: None,
            executable_path: None,
            content_hash: None,
            trusted_hash: None,
            trust_state: asset_core::domain::ConversationAdapterTrustState::Trusted,
            protocol_version: Some(1),
            capabilities: vec!["list".to_string()],
            input_kinds: vec![asset_core::domain::ConversationSourceKind::Directory],
            card_contract_version: Some(1),
            card_kinds: Vec::new(),
            projection_version: Some(1),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        let source = asset_core::domain::ConversationSource {
            id: "test-source-unproc".to_string(),
            adapter_id: "test-adapter-unproc".to_string(),
            name: "Test Source".to_string(),
            kind: asset_core::domain::ConversationSourceKind::Directory,
            location: temp_dir.path().to_string_lossy().to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        };

        asset_core::store::upsert_conversation_adapter_sqlx(&pool, tenant_id, &adapter)
            .await
            .expect("upsert adapter");
        asset_core::store::upsert_conversation_source_sqlx(&pool, tenant_id, &source)
            .await
            .expect("upsert source");

        let make_turn =
            |idx: i64, q: &str, a: &str| asset_core::domain::NormalizedConversationTurn {
                external_id: Some(format!("turn-{idx}")),
                turn_index: idx,
                user_text: q.to_string(),
                parts: vec![asset_core::domain::NormalizedConversationPart {
                    role: asset_core::domain::ConversationPartRole::Assistant,
                    kind: asset_core::domain::ConversationPartKind::Text,
                    text: Some(a.to_string()),
                    language: None,
                    command: None,
                    cwd: None,
                    status: None,
                    exit_code: None,
                    command_label: None,
                    source_execution_id: None,
                    content_card: None,
                    metadata_json: None,
                }],
                ..Default::default()
            };

        let session_a = asset_core::domain::NormalizedConversationSession {
            external_id: "sess-a-ext".to_string(),
            title: Some("Session A".to_string()),
            project_path: Some(project_path.to_string()),
            turns: vec![make_turn(0, "A1", "Ans A1"), make_turn(1, "A2", "Ans A2")],
            ..Default::default()
        };

        let session_b = asset_core::domain::NormalizedConversationSession {
            external_id: "sess-b-ext".to_string(),
            title: Some("Session B".to_string()),
            project_path: Some(project_path.to_string()),
            turns: vec![
                make_turn(0, "B1", "Ans B1"),
                make_turn(1, "B2", "Ans B2"),
                make_turn(2, "B3", "Ans B3"),
            ],
            ..Default::default()
        };

        asset_core::store::import_conversation_sessions_sqlx(
            &pool,
            tenant_id,
            &source,
            &[session_a, session_b],
            false,
        )
        .await
        .expect("import sessions");
    });

    let mut cmd_init = aiwc_command();
    let assert_init = cmd_init
        .env("ASSETIWEAVE_DB_PATH", &db_path)
        .args(["conversation", "list", "--format", "json"])
        .assert()
        .success();
    let init_json: Value = serde_json::from_slice(&assert_init.get_output().stdout).unwrap();
    let init_items = init_json["data"].as_array().unwrap();
    let sess_a_id = init_items
        .iter()
        .find(|i| i["latest_user_prompt"].as_str() == Some("A2"))
        .unwrap()["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let sess_b_id = init_items
        .iter()
        .find(|i| i["latest_user_prompt"].as_str() == Some("B3"))
        .unwrap()["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Write watermarks to .state/projects/<slug>.json in mem_dir
    let storage = asset_core::infrastructure::memory::MemoryStorage::new_with_root(
        mem_dir.path().to_path_buf(),
        "default",
    );
    storage.init_dirs().unwrap();
    let mut proj_state = asset_core::domain::memory::ProjectMemoryState::new(&project_slug);
    proj_state.set_watermark(&sess_a_id, 2);
    proj_state.set_watermark(&sess_b_id, 1);
    storage
        .write_project_state_atomic(&project_slug, &proj_state)
        .unwrap();

    // 1. Regular list --format json: both sessions returned with watermark view models
    let mut cmd_list = aiwc_command();
    let assert_list = cmd_list
        .env("ASSETIWEAVE_DB_PATH", &db_path)
        .env("ASSETIWEAVE_MEMORY_ROOT", mem_dir.path())
        .args(["conversation", "list", "--format", "json"])
        .assert()
        .success();

    let output_json =
        String::from_utf8(assert_list.get_output().stdout.clone()).expect("valid utf8");
    let res_json: Value = serde_json::from_str(&output_json).expect("valid json list");
    assert_eq!(res_json["ok"], true);
    let items = res_json["data"].as_array().expect("items is array");
    assert_eq!(items.len(), 2);

    let item_a = items
        .iter()
        .find(|item| item["session_id"].as_str() == Some(&sess_a_id))
        .expect("item a found");
    assert_eq!(item_a["project_slug"], project_slug);
    assert_eq!(item_a["turn_count"], 2);
    assert_eq!(item_a["watermark"], 2);
    assert_eq!(item_a["unprocessed_turns"], 0);

    let item_b = items
        .iter()
        .find(|item| item["session_id"].as_str() == Some(&sess_b_id))
        .expect("item b found");
    assert_eq!(item_b["project_slug"], project_slug);
    assert_eq!(item_b["turn_count"], 3);
    assert_eq!(item_b["watermark"], 1);
    assert_eq!(item_b["unprocessed_turns"], 2);

    // 2. Unprocessed filter list: only session B should appear
    let mut cmd_unprocessed = aiwc_command();
    let assert_unprocessed = cmd_unprocessed
        .env("ASSETIWEAVE_DB_PATH", &db_path)
        .env("ASSETIWEAVE_MEMORY_ROOT", mem_dir.path())
        .args(["conversation", "list", "--unprocessed", "--format", "json"])
        .assert()
        .success();

    let output_unproc =
        String::from_utf8(assert_unprocessed.get_output().stdout.clone()).expect("valid utf8");
    let res_unproc: Value = serde_json::from_str(&output_unproc).expect("valid json list");
    assert_eq!(res_unproc["ok"], true);
    let items_unproc = res_unproc["data"].as_array().expect("items is array");
    assert_eq!(
        items_unproc.len(),
        1,
        "Only session B should be unprocessed"
    );
    assert_eq!(items_unproc[0]["session_id"], sess_b_id);
}

#[test]
fn test_conversation_batch_outline_and_qa_digest_cli() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("batch_outline_test.db");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let pool = asset_core::store::open_migrated_pool(&db_path)
            .await
            .expect("open migrated pool");
        let tenant_id = "default";
        let now = "2026-11-04T00:00:00Z".to_string();

        let adapter = asset_core::domain::ConversationAdapter {
            id: "test-adapter-batch".to_string(),
            name: "Test Adapter".to_string(),
            kind: asset_core::domain::ConversationAdapterKind::External,
            version: "0.1.0".to_string(),
            enabled: true,
            manifest_path: None,
            executable_path: None,
            content_hash: None,
            trusted_hash: None,
            trust_state: asset_core::domain::ConversationAdapterTrustState::Trusted,
            protocol_version: Some(1),
            capabilities: vec!["list".to_string()],
            input_kinds: vec![asset_core::domain::ConversationSourceKind::Directory],
            card_contract_version: Some(1),
            card_kinds: Vec::new(),
            projection_version: Some(1),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        let source = asset_core::domain::ConversationSource {
            id: "test-source-batch".to_string(),
            adapter_id: "test-adapter-batch".to_string(),
            name: "Test Source".to_string(),
            kind: asset_core::domain::ConversationSourceKind::Directory,
            location: temp_dir.path().to_string_lossy().to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        };

        asset_core::store::upsert_conversation_adapter_sqlx(&pool, tenant_id, &adapter)
            .await
            .expect("upsert adapter");
        asset_core::store::upsert_conversation_source_sqlx(&pool, tenant_id, &source)
            .await
            .expect("upsert source");

        let long_answer: String = (0..200)
            .map(|_| 'M')
            .chain((0..50).map(|_| 'N'))
            .chain((0..300).map(|_| 'O'))
            .collect();

        let session1 = asset_core::domain::NormalizedConversationSession {
            external_id: "batch-s1-ext".to_string(),
            title: Some("Batch S1".to_string()),
            turns: vec![asset_core::domain::NormalizedConversationTurn {
                external_id: Some("turn-s1-1".to_string()),
                turn_index: 0,
                user_text: "What is machine learning?".to_string(),
                parts: vec![asset_core::domain::NormalizedConversationPart {
                    role: asset_core::domain::ConversationPartRole::Assistant,
                    kind: asset_core::domain::ConversationPartKind::Text,
                    text: Some(long_answer),
                    language: None,
                    command: None,
                    cwd: None,
                    status: None,
                    exit_code: None,
                    command_label: None,
                    source_execution_id: None,
                    content_card: None,
                    metadata_json: None,
                }],
                ..Default::default()
            }],
            ..Default::default()
        };

        let session2 = asset_core::domain::NormalizedConversationSession {
            external_id: "batch-s2-ext".to_string(),
            title: Some("Batch S2".to_string()),
            turns: vec![asset_core::domain::NormalizedConversationTurn {
                external_id: Some("turn-s2-1".to_string()),
                turn_index: 0,
                user_text: "Hello".to_string(),
                parts: vec![asset_core::domain::NormalizedConversationPart {
                    role: asset_core::domain::ConversationPartRole::Assistant,
                    kind: asset_core::domain::ConversationPartKind::Text,
                    text: Some("World".to_string()),
                    language: None,
                    command: None,
                    cwd: None,
                    status: None,
                    exit_code: None,
                    command_label: None,
                    source_execution_id: None,
                    content_card: None,
                    metadata_json: None,
                }],
                ..Default::default()
            }],
            ..Default::default()
        };

        asset_core::store::import_conversation_sessions_sqlx(
            &pool,
            tenant_id,
            &source,
            &[session1, session2],
            false,
        )
        .await
        .expect("import sessions");
    });

    let mut cmd_init = aiwc_command();
    let assert_init = cmd_init
        .env("ASSETIWEAVE_DB_PATH", &db_path)
        .args(["conversation", "list", "--format", "json"])
        .assert()
        .success();
    let init_json: Value = serde_json::from_slice(&assert_init.get_output().stdout).unwrap();
    let init_items = init_json["data"].as_array().unwrap();
    let s1_id = init_items
        .iter()
        .find(|i| i["latest_user_prompt"].as_str() == Some("What is machine learning?"))
        .unwrap()["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let s2_id = init_items
        .iter()
        .find(|i| i["latest_user_prompt"].as_str() == Some("Hello"))
        .unwrap()["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    // 1. Single ID: returns single-item envelope {"ok": true, "data": {"level": "session", ...}}
    let mut cmd_single = aiwc_command();
    let assert_single = cmd_single
        .env("ASSETIWEAVE_DB_PATH", &db_path)
        .args(["conversation", "outline", &s1_id])
        .assert()
        .success();
    let out_single: Value = serde_json::from_slice(&assert_single.get_output().stdout).unwrap();
    assert_eq!(out_single["ok"], true);
    assert_eq!(out_single["data"]["level"], "session");

    // 2. Batch outline with multiple IDs: {"ok": true, "data": {"level": "batch", "items": [...], "truncated": false}}
    let mut cmd_batch = aiwc_command();
    let assert_batch = cmd_batch
        .env("ASSETIWEAVE_DB_PATH", &db_path)
        .args(["conversation", "outline", &s1_id, &s2_id])
        .assert()
        .success();
    let out_batch: Value = serde_json::from_slice(&assert_batch.get_output().stdout).unwrap();
    assert_eq!(out_batch["ok"], true);
    assert_eq!(out_batch["data"]["level"], "batch");
    assert_eq!(out_batch["data"]["items"].as_array().unwrap().len(), 2);
    assert_eq!(out_batch["data"]["truncated"], false);

    // 3. QA Digest flag: --qa-digest truncates answer > 500 characters
    let mut cmd_digest = aiwc_command();
    let assert_digest = cmd_digest
        .env("ASSETIWEAVE_DB_PATH", &db_path)
        .args(["conversation", "outline", &s1_id, "--qa-digest"])
        .assert()
        .success();
    let out_digest: Value = serde_json::from_slice(&assert_digest.get_output().stdout).unwrap();
    assert_eq!(out_digest["ok"], true);
    assert_eq!(out_digest["data"]["level"], "session");
    let ans_val = out_digest["data"]["turns"]["rows"][0][3].as_str().unwrap();
    assert!(ans_val.contains("... [省略 50 字] ..."));
}
