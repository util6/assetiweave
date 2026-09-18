use super::*;
use crate::backend::agents::types::{
    AgentDefinition, AgentEnvEntry, AgentId, AgentProtocol, DeclaredAgentCapabilities,
};
use std::{
    env,
    io::{Read, Write},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn process_fixture() {
    match env::var("ASSETIWEAVE_MANAGED_PROCESS_FIXTURE").as_deref() {
        Ok("large-stderr") => {
            std::io::stderr().write_all(&vec![b'e'; 64 * 1024]).unwrap();
        }
        Ok("broken-stderr") => {
            std::io::stderr().write_all(&[0xff, b'a', 0xfe]).unwrap();
        }
        Ok("echo") => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input).unwrap();
            write!(std::io::stdout(), "echo:{input}").unwrap();
        }
        Ok("env-overlay") => {
            write!(
                std::io::stdout(),
                "overlay:{}",
                env::var("ASSETIWEAVE_TEST_OVERLAY").unwrap_or_default()
            )
            .unwrap();
        }
        #[cfg(unix)]
        Ok("ignore-term") => {
            // SAFETY: this fixture intentionally ignores SIGTERM so the
            // parent test can prove the SIGKILL fallback converges.
            unsafe {
                libc::signal(libc::SIGTERM, libc::SIG_IGN);
            }
            std::fs::write(
                env::var("ASSETIWEAVE_MANAGED_PROCESS_PID_FILE").unwrap(),
                std::process::id().to_string(),
            )
            .expect("write ignore-term readiness pid");
            std::thread::sleep(Duration::from_secs(5));
        }
        Ok("grandchild") | Ok("launcher-exits") => {
            let mode = env::var("ASSETIWEAVE_MANAGED_PROCESS_FIXTURE").unwrap();
            let mut child = std::process::Command::new(
                env::current_exe().expect("resolve grandchild fixture binary"),
            )
            .args([
                "--exact",
                "backend::agents::process::tests::process_fixture",
                "--nocapture",
            ])
            .env("ASSETIWEAVE_MANAGED_PROCESS_FIXTURE", "grandchild-leaf")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn grandchild fixture");
            std::fs::write(
                env::var("ASSETIWEAVE_MANAGED_PROCESS_PID_FILE").unwrap(),
                child.id().to_string(),
            )
            .expect("write grandchild pid");
            if mode == "grandchild" {
                std::thread::sleep(Duration::from_secs(5));
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        Ok("grandchild-leaf") => std::thread::sleep(Duration::from_secs(5)),
        Ok("sleep") => std::thread::sleep(Duration::from_secs(5)),
        _ => {}
    }
}

#[tokio::test(flavor = "current_thread")]
async fn first_stdio_take_supports_bidirectional_io() {
    let process = spawn_fixture("echo", 1024).await;
    let (mut stdin, mut stdout) = process.take_stdio().await.expect("first stdio take");

    stdin.write_all(b"PING").await.expect("write child stdin");
    stdin.shutdown().await.expect("close child stdin");
    drop(stdin);
    let mut output = String::new();
    tokio::time::timeout(Duration::from_secs(3), stdout.read_to_string(&mut output))
        .await
        .expect("stdout read in time")
        .expect("read child stdout");

    assert!(output.contains("echo:PING"));
    assert!(process.wait_for_exit().await.is_some());
}

#[tokio::test(flavor = "current_thread")]
async fn stdio_can_only_be_taken_once() {
    let process = spawn_fixture("sleep", 1024).await;

    let first = process.take_stdio().await;
    let second = process.take_stdio().await;

    assert!(first.is_ok());
    assert!(matches!(
        second,
        Err(ManagedAgentProcessError::StdioAlreadyTaken)
    ));
    process.force_kill_tree().await;
}

#[tokio::test(flavor = "current_thread")]
async fn stderr_is_drained_into_a_bounded_tail() {
    let process = spawn_fixture("large-stderr", 1024).await;
    tokio::time::timeout(Duration::from_secs(3), process.wait_for_exit())
        .await
        .expect("fixture exits in time")
        .expect("exit snapshot");
    assert!(process.wait_for_stderr_eof(Duration::from_secs(1)).await);

    let tail = process.stderr_tail().expect("stderr tail");
    assert_eq!(tail.bytes.len(), 1024);
    assert!(tail.truncated);
}

#[tokio::test(flavor = "current_thread")]
async fn broken_utf8_stderr_has_a_lossy_diagnostic() {
    let process = spawn_fixture("broken-stderr", 1024).await;
    tokio::time::timeout(Duration::from_secs(3), process.wait_for_exit())
        .await
        .expect("fixture exits in time")
        .expect("exit snapshot");
    assert!(process.wait_for_stderr_eof(Duration::from_secs(1)).await);

    let diagnostic = process.stderr_tail().unwrap().lossy_text();

    assert!(diagnostic.contains('�'));
    assert!(diagnostic.contains('a'));
}

#[tokio::test(flavor = "current_thread")]
async fn immediate_exit_is_published_with_status() {
    let process = spawn_fixture("immediate-exit", 1024).await;

    let exit = tokio::time::timeout(Duration::from_secs(3), process.wait_for_exit())
        .await
        .expect("fixture exits in time")
        .expect("exit snapshot");

    assert!(exit.success);
    assert_eq!(exit.code, Some(0));
    assert!(exit.wait_error.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn exit_is_watched_and_termination_is_idempotent() {
    let process = spawn_fixture("sleep", 1024).await;

    let first = tokio::time::timeout(
        Duration::from_secs(3),
        process.terminate(Duration::from_millis(100)),
    )
    .await
    .expect("first termination in time");
    let second = tokio::time::timeout(
        Duration::from_secs(3),
        process.terminate(Duration::from_millis(100)),
    )
    .await
    .expect("second termination in time");

    assert!(first.exit.is_some());
    assert!(second.exit.is_some());
}

#[tokio::test(flavor = "current_thread")]
#[cfg(unix)]
async fn graceful_deadline_converges_through_force_kill() {
    let process = spawn_fixture("sleep", 1024).await;

    let report = tokio::time::timeout(
        Duration::from_secs(3),
        process.terminate(Duration::from_millis(50)),
    )
    .await
    .expect("termination in time");

    assert!(report.terminate_requested);
    assert!(report.force_kill_requested);
    assert!(report.exit.is_some());
    assert!(report.signal_errors.is_empty());
}

#[tokio::test(flavor = "current_thread")]
#[cfg(unix)]
async fn sigkill_fallback_stops_a_process_that_ignores_sigterm() {
    let pid_file = temp_pid_file();
    let process = spawn_tree_fixture("ignore-term", &pid_file).await;
    let _ready_pid = read_pid_file(&pid_file).await;
    let started = std::time::Instant::now();

    let report = tokio::time::timeout(
        Duration::from_secs(3),
        process.terminate(Duration::from_millis(75)),
    )
    .await
    .expect("termination in time");

    assert!(started.elapsed() >= Duration::from_millis(60));
    assert!(report.exit.is_some());
    assert!(!report.exit.unwrap().success);
    let _ = std::fs::remove_file(pid_file);
}

#[tokio::test(flavor = "current_thread")]
async fn force_kill_stops_the_direct_child_and_grandchild() {
    let pid_file = temp_pid_file();
    let process = spawn_tree_fixture("grandchild", &pid_file).await;
    let direct_pid = process.process_id();
    let grandchild_pid = read_pid_file(&pid_file).await;

    let report = tokio::time::timeout(Duration::from_secs(3), process.force_kill_tree())
        .await
        .expect("force kill in time");

    assert!(report.exit.is_some());
    wait_until_process_is_gone(direct_pid).await;
    wait_until_process_is_gone(grandchild_pid).await;
    let _ = std::fs::remove_file(pid_file);
}

#[tokio::test(flavor = "current_thread")]
#[cfg(unix)]
async fn recorded_group_is_cleaned_after_the_launcher_exits() {
    let pid_file = temp_pid_file();
    let process = spawn_tree_fixture("launcher-exits", &pid_file).await;
    let grandchild_pid = read_pid_file(&pid_file).await;
    tokio::time::timeout(Duration::from_secs(3), process.wait_for_exit())
        .await
        .expect("launcher exits in time")
        .expect("launcher exit snapshot");
    assert!(process_exists(grandchild_pid));

    tokio::time::timeout(
        Duration::from_secs(3),
        process.terminate(Duration::from_millis(50)),
    )
    .await
    .expect("tree cleanup in time");

    wait_until_process_is_gone(grandchild_pid).await;
    let _ = std::fs::remove_file(pid_file);
}

#[tokio::test(flavor = "current_thread")]
async fn environment_overlay_is_applied_without_entering_the_preview() {
    let mut definition = fixture_definition("env-overlay");
    definition.env.push(AgentEnvEntry::new(
        "ASSETIWEAVE_TEST_OVERLAY",
        "SECRET_OVERLAY_VALUE",
    ));
    let preview = format!("{:?}", SafeSpawnPreview::from_definition(&definition, None));
    let process = ManagedAgentProcess::spawn(&definition, None, 1024)
        .await
        .expect("spawn fixture");
    let (stdin, mut stdout) = process.take_stdio().await.expect("take stdio");
    drop(stdin);
    let mut output = String::new();
    tokio::time::timeout(Duration::from_secs(3), stdout.read_to_string(&mut output))
        .await
        .expect("stdout read in time")
        .expect("read child stdout");

    assert!(output.contains("overlay:SECRET_OVERLAY_VALUE"));
    assert!(!preview.contains("SECRET_OVERLAY_VALUE"));
}

#[test]
fn spawn_preview_does_not_expose_argument_or_environment_values() {
    let mut definition = fixture_definition("SECRET_ENV_VALUE");
    definition.args.push("SECRET_ARGUMENT".to_string());

    let preview = SafeSpawnPreview::from_definition(&definition, None);
    let debug = format!("{preview:?}");

    assert!(!debug.contains("SECRET_ARGUMENT"));
    assert!(!debug.contains("SECRET_ENV_VALUE"));
    assert!(debug.contains("ASSETIWEAVE_MANAGED_PROCESS_FIXTURE"));
}

async fn spawn_fixture(mode: &str, stderr_cap: usize) -> ManagedAgentProcess {
    tokio::time::timeout(
        Duration::from_secs(3),
        ManagedAgentProcess::spawn(&fixture_definition(mode), None, stderr_cap),
    )
    .await
    .expect("spawn in time")
    .expect("spawn fixture")
}

async fn spawn_tree_fixture(mode: &str, pid_file: &Path) -> ManagedAgentProcess {
    let mut definition = fixture_definition(mode);
    definition.env.push(AgentEnvEntry::new(
        "ASSETIWEAVE_MANAGED_PROCESS_PID_FILE",
        pid_file.to_string_lossy(),
    ));
    tokio::time::timeout(
        Duration::from_secs(3),
        ManagedAgentProcess::spawn(&definition, None, 1024),
    )
    .await
    .expect("tree spawn in time")
    .expect("spawn tree fixture")
}

fn temp_pid_file() -> PathBuf {
    env::temp_dir().join(format!(
        "assetiweave-managed-process-{}.pid",
        uuid::Uuid::new_v4()
    ))
}

async fn read_pid_file(path: &Path) -> u32 {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(value) = std::fs::read_to_string(path) {
                if let Ok(pid) = value.trim().parse() {
                    break pid;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("grandchild pid file in time")
}

async fn wait_until_process_is_gone(process_id: u32) {
    tokio::time::timeout(Duration::from_secs(2), async move {
        while process_exists(process_id) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("process exits in time");
}

#[cfg(unix)]
fn process_exists(process_id: u32) -> bool {
    // SAFETY: signal zero performs existence/permission checking only.
    let result = unsafe { libc::kill(process_id as libc::pid_t, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn process_exists(process_id: u32) -> bool {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {process_id}"), "/FO", "CSV", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    output.is_ok_and(|output| {
        String::from_utf8_lossy(&output.stdout).lines().any(|line| {
            line.split(',')
                .nth(1)
                .is_some_and(|pid| pid.trim().trim_matches('"') == process_id.to_string())
        })
    })
}

fn fixture_definition(mode: &str) -> AgentDefinition {
    AgentDefinition {
        id: AgentId::parse("fixture").unwrap(),
        installation_id: None,
        display_name: "Fixture".to_string(),
        protocol: AgentProtocol::Acp,
        command: env::current_exe()
            .expect("resolve test binary")
            .to_string_lossy()
            .into_owned(),
        args: vec![
            "--exact".to_string(),
            "backend::agents::process::tests::process_fixture".to_string(),
            "--nocapture".to_string(),
        ],
        env: vec![AgentEnvEntry::new(
            "ASSETIWEAVE_MANAGED_PROCESS_FIXTURE",
            mode,
        )],
        declared_capabilities: DeclaredAgentCapabilities::acp_text(),
        availability_probe: None,
        model_discovery: None,
        session_cleanup: None,
        session_cleanup_not_found_markers: Vec::new(),
    }
}
