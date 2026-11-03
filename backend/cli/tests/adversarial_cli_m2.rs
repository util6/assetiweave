use assert_cmd::Command;
use serde_json::Value;

thread_local! {
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

// =========================================================================
// CLI Multibyte UTF-8 Safety Stress Test
// =========================================================================

#[test]
fn test_cli_utf8_multibyte_session_id_commit_and_watermark() {
    let tmp = tempfile::tempdir().unwrap();
    let memory_root = tmp.path().join("memories");
    std::fs::create_dir_all(&memory_root).unwrap();

    let test_cases = vec![
        ("会话测试-12345678", "Chinese CJK 3-byte char boundary"),
        ("🚀🌟🔥🤖💻🎉✨🔧", "4-byte unicode emojis"),
        ("短", "Short 1-char multibyte string"),
        ("abc", "Short ASCII string (< 8 chars)"),
        ("混合-🚀-mixed-123", "Mixed multibyte and ASCII"),
    ];

    for (session_id, desc) in test_cases {
        println!(
            "Testing CLI UTF-8 safety with session_id: {} ({})",
            session_id, desc
        );

        // 1. Check watermark with multibyte session ID
        let mut cmd_chk = aiwc_command();
        let assert_chk = cmd_chk
            .env("ASSETIWEAVE_MEMORY_ROOT", &memory_root)
            .args([
                "memory",
                "watermark",
                "check",
                "--project",
                "cli-utf8-proj",
                "--session-id",
                session_id,
                "--format",
                "json",
            ])
            .assert()
            .success();

        let out_chk = String::from_utf8(assert_chk.get_output().stdout.clone()).unwrap();
        let json_chk: Value = serde_json::from_str(&out_chk).unwrap();
        assert_eq!(json_chk["ok"], true, "Watermark check failed for {desc}");

        // 2. Advance watermark with multibyte session ID
        let mut cmd_adv = aiwc_command();
        let assert_adv = cmd_adv
            .env("ASSETIWEAVE_MEMORY_ROOT", &memory_root)
            .args([
                "memory",
                "watermark",
                "advance",
                "--project",
                "cli-utf8-proj",
                "--session-id",
                session_id,
                "--turn",
                "7",
                "--format",
                "json",
            ])
            .assert()
            .success();

        let out_adv = String::from_utf8(assert_adv.get_output().stdout.clone()).unwrap();
        let json_adv: Value = serde_json::from_str(&out_adv).unwrap();
        assert_eq!(json_adv["ok"], true, "Watermark advance failed for {desc}");
        assert_eq!(json_adv["data"]["advanced_to_turn"], 7);

        // 3. Commit evolution with multibyte session ID
        // (Without providing --short-id, so CLI internally extracts short_id from session_id!)
        let mut cmd_commit = aiwc_command();
        let assert_commit = cmd_commit
            .env("ASSETIWEAVE_MEMORY_ROOT", &memory_root)
            .args([
                "memory",
                "commit",
                "--project",
                "cli-utf8-proj",
                "--topic",
                "cli-utf8-topic",
                "--topic-name",
                "CLI UTF8 Stress Topic",
                "--summary",
                &format!("Commit for {}", desc),
                "--session-id",
                session_id,
                "--turn",
                "7",
                "--status",
                "completed",
                "--format",
                "json",
            ])
            .assert()
            .success();

        let out_commit = String::from_utf8(assert_commit.get_output().stdout.clone()).unwrap();
        let json_commit: Value = serde_json::from_str(&out_commit).unwrap();
        assert_eq!(
            json_commit["ok"], true,
            "Commit failed for session_id {} ({})",
            session_id, desc
        );
        let committed_id = json_commit["data"]["committed_node_id"].as_str().unwrap();
        assert!(
            committed_id.contains("#T7"),
            "Committed node ID must contain #T7: {}",
            committed_id
        );
    }
}
