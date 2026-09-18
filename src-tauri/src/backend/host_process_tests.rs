use super::*;
use std::{
    env,
    io::{self, Write},
    time::{Duration, Instant},
};

#[tokio::test(flavor = "current_thread")]
async fn process_fixture() {
    match env::var("ASSETIWEAVE_HOST_PROCESS_FIXTURE").as_deref() {
        Ok("large-output") => {
            io::stdout().write_all(&vec![b'x'; 256 * 1024]).unwrap();
        }
        Ok("timeout") => {
            let (_tx, rx) = std::sync::mpsc::channel::<()>();
            let _ = rx.recv_timeout(Duration::from_secs(5));
        }
        #[cfg(unix)]
        Ok("launcher-exits") => {
            let _ = tokio::process::Command::new("sh")
                .args(["-c", "sleep 5"])
                .spawn()
                .expect("spawn inherited-pipe descendant");
        }
        #[cfg(windows)]
        Ok("launcher-exits") => {
            let _ = tokio::process::Command::new("cmd")
                .args(["/C", "ping -n 6 127.0.0.1 > nul"])
                .spawn()
                .expect("spawn inherited-pipe descendant");
        }
        Ok("normal-exit") => {
            let _ = io::stdout().write_all(b"fixture-stdout-content");
            let _ = io::stderr().write_all(b"fixture-stderr-content");
        }
        Ok("lines-output") => {
            let _ = io::stdout().write_all(b"line-1\nline-2\nline-3\n");
        }
        Ok("nonzero-exit") => {
            let _ = io::stderr().write_all(b"exiting with error 42");
            std::process::exit(42);
        }
        Ok("ignore-term") => {
            #[cfg(unix)]
            unsafe {
                libc::signal(libc::SIGTERM, libc::SIG_IGN);
            }
            let _ = io::stdout().write_all(b"ready\n");
            let _ = io::stdout().flush();
            let (_tx, rx) = std::sync::mpsc::channel::<()>();
            let _ = rx.recv_timeout(Duration::from_secs(10));
        }
        _ => {}
    }
}

fn fixture_spec(
    mode: &str,
    timeout: Duration,
    stdout_limit: usize,
    stderr_limit: usize,
) -> HostCommandSpec {
    HostCommandSpec {
        program: env::current_exe().expect("resolve test binary"),
        args: vec![
            "--exact".to_string(),
            "backend::host_process::tests::process_fixture".to_string(),
            "--nocapture".to_string(),
        ],
        env: vec![(
            "ASSETIWEAVE_HOST_PROCESS_FIXTURE".to_string(),
            mode.to_string(),
        )],
        working_dir: None,
        stdin: HostInput::Null,
        timeout,
        stdout_limit,
        stderr_limit,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn async_runner_shares_bounded_output_and_reports_elapsed_time() {
    let output = run_host_command(
        fixture_spec("large-output", Duration::from_secs(5), 32 * 1024, 32 * 1024),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("async host command should exit");

    assert!(output.status.success());
    assert_eq!(output.stdout.len(), 32 * 1024);
    assert!(output.stdout_truncated);
    assert!(!output.elapsed.is_zero());
}

#[tokio::test(flavor = "current_thread")]
async fn async_runner_cancels_and_reaps_the_process_tree() {
    let cancellation = tokio_util::sync::CancellationToken::new();
    let task = run_host_command(
        fixture_spec("timeout", Duration::from_secs(5), 32 * 1024, 32 * 1024),
        cancellation.clone(),
    );
    tokio::pin!(task);
    tokio::time::sleep(Duration::from_millis(50)).await;
    cancellation.cancel();

    let error = task.await.expect_err("cancelled command should fail");
    assert!(matches!(error, HostProcessError::Cancelled));
}

#[test]
fn token_cancellation_has_no_mirror_watcher_thread() {
    let source = include_str!("host_process.rs");
    assert!(!source.contains(concat!("let watcher_", "done =")));
    assert!(!source.contains(concat!("let watcher_", "token =")));
}

#[tokio::test(flavor = "current_thread")]
async fn public_seam_host_command_normal_exit() {
    let output = run_host_command(
        fixture_spec("normal-exit", Duration::from_secs(5), 32 * 1024, 32 * 1024),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("normal exit succeeds");

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("fixture-stdout-content"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("fixture-stderr-content"));
}

#[tokio::test(flavor = "current_thread")]
async fn public_seam_host_command_nonzero_exit() {
    let output = run_host_command(
        fixture_spec("nonzero-exit", Duration::from_secs(5), 32 * 1024, 32 * 1024),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("runs to nonzero completion");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(42));
    assert_eq!(output.stderr, b"exiting with error 42");
}

#[test]
fn host_process_error_into_app_error_mapping() {
    use crate::backend::runtime::AppError;

    let missing = HostProcessError::MissingProgram {
        program: PathBuf::from("nonexistent-tool"),
    };
    let app_err: AppError = missing.into();
    assert!(matches!(app_err, AppError::NotFound(_)));

    let timeout = HostProcessError::Timeout {
        stdout: Vec::new(),
        stderr: Vec::new(),
        stdout_truncated: false,
        stderr_truncated: false,
    };
    let app_err: AppError = timeout.into();
    assert!(matches!(app_err, AppError::Timeout(_)));

    let cancelled = HostProcessError::Cancelled;
    let app_err: AppError = cancelled.into();
    assert!(matches!(app_err, AppError::Cancelled(_)));

    let limit = HostProcessError::OutputLimitExceeded {
        stdout: true,
        stderr: false,
    };
    let app_err: AppError = limit.into();
    assert!(matches!(app_err, AppError::Process(_)));
}

// =========================================================================
// C-PROCESS-01 & B2-P01/B2-P03C: Contract Tests for process-wrap with production async runner
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn contract_process_wrap_normal_exit() {
    let output = run_host_command_async(
        fixture_spec("normal-exit", Duration::from_secs(5), 32 * 1024, 32 * 1024),
        None,
    )
    .await
    .expect("normal exit fixture should succeed");

    assert!(output.status.success());
    assert_eq!(output.status.code(), Some(0));
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let stderr_str = String::from_utf8_lossy(&output.stderr);
    assert!(stdout_str.contains("fixture-stdout-content"));
    assert!(stderr_str.contains("fixture-stderr-content"));
    assert!(!output.stdout_truncated);
    assert!(!output.stderr_truncated);
}

#[tokio::test(flavor = "current_thread")]
async fn contract_process_wrap_nonzero_exit() {
    let output = run_host_command_async(
        fixture_spec("nonzero-exit", Duration::from_secs(5), 32 * 1024, 32 * 1024),
        None,
    )
    .await
    .expect("nonzero exit fixture completes execution");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(42));
    assert_eq!(output.stderr, b"exiting with error 42");
}

#[tokio::test(flavor = "current_thread")]
async fn contract_process_wrap_bounded_output() {
    let output = run_host_command_async(
        fixture_spec("large-output", Duration::from_secs(5), 32 * 1024, 32 * 1024),
        None,
    )
    .await
    .expect("large-output fixture should succeed");

    assert!(output.status.success());
    assert_eq!(output.stdout.len(), 32 * 1024);
    assert!(output.stdout_truncated);
}

#[tokio::test(flavor = "current_thread")]
async fn contract_process_wrap_timeout() {
    let started = Instant::now();
    let err = run_host_command_async(
        fixture_spec("timeout", Duration::from_millis(150), 32 * 1024, 32 * 1024),
        None,
    )
    .await
    .expect_err("timeout fixture must fail with timeout");

    assert!(matches!(err, HostProcessError::Timeout { .. }));
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "timeout reap must be prompt"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn contract_process_wrap_pre_cancel() {
    let cancellation = tokio_util::sync::CancellationToken::new();
    cancellation.cancel();

    let err = run_host_command_async(
        fixture_spec("timeout", Duration::from_secs(5), 32 * 1024, 32 * 1024),
        Some(&cancellation),
    )
    .await
    .expect_err("pre-cancelled command must fail immediately");

    assert!(matches!(err, HostProcessError::Cancelled));
}

#[tokio::test(flavor = "current_thread")]
async fn contract_process_wrap_mid_flight_cancel() {
    let cancellation = tokio_util::sync::CancellationToken::new();
    let cancel_handle = cancellation.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel_handle.cancel();
    });

    let started = Instant::now();
    let err = run_host_command_async(
        fixture_spec("timeout", Duration::from_secs(5), 32 * 1024, 32 * 1024),
        Some(&cancellation),
    )
    .await
    .expect_err("mid-flight cancelled command must fail");

    assert!(matches!(err, HostProcessError::Cancelled));
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "cancel must reap immediately"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn contract_process_wrap_descendant_held_pipe() {
    let started = Instant::now();
    let output = run_host_command_async(
        fixture_spec(
            "launcher-exits",
            Duration::from_secs(4),
            64 * 1024,
            64 * 1024,
        ),
        None,
    )
    .await
    .expect("launcher exit should succeed without hanging on descendant pipes");

    assert!(
        output.status.success(),
        "launcher itself must have exited successfully"
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "must reap descendant and pipe without waiting full 5s"
    );
}

#[tokio::test(flavor = "current_thread")]
#[cfg(windows)]
async fn contract_windows_job_object_reaps_descendant_held_pipe() {
    let started = Instant::now();
    let output = run_host_command_async(
        fixture_spec(
            "launcher-exits",
            Duration::from_secs(4),
            64 * 1024,
            64 * 1024,
        ),
        None,
    )
    .await
    .expect("launcher exit should succeed without hanging on descendant pipes via JobObject");

    assert!(
        output.status.success(),
        "launcher itself must have exited successfully"
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "JobObject must reap descendant and pipe without waiting full 5s"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn contract_process_wrap_ignored_graceful_termination() {
    let cmd = make_tokio_fixture_command("ignore-term");
    let mut wrap = wrap_tokio_command(cmd);
    let mut child = wrap.spawn().expect("spawn ignore-term fixture");

    if let Some(stdout) = child.stdout().take() {
        use tokio::io::AsyncBufReadExt;
        let mut reader = tokio::io::BufReader::new(stdout);
        let mut line = String::new();
        while let Ok(n) = reader.read_line(&mut line).await {
            if n == 0 || line.contains("ready") {
                break;
            }
            line.clear();
        }
    }

    #[cfg(unix)]
    {
        let _ = child.signal(libc::SIGTERM);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let status = child.try_wait().expect("try_wait");
        assert!(status.is_none(), "process should have ignored SIGTERM");
    }

    let started = Instant::now();
    let _ = child.start_kill();
    let status = child.wait().await.expect("wait for force killed child");
    assert!(
        !status.success(),
        "force killed process must not be success"
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "force kill should be fast"
    );
}

#[test]
fn contract_which_standard_lookup_and_desktop_fallback() {
    let unique_dir_name = format!("assetiweave-which-test-{}", uuid::Uuid::new_v4());
    let root_temp = env::temp_dir().join(unique_dir_name);
    let bin_dir = root_temp.join("bin");
    let work_dir = root_temp.join("work");
    fs::create_dir_all(&bin_dir).expect("create bin dir");
    fs::create_dir_all(&work_dir).expect("create work dir");

    let exe_name = host_executable_name("test-tool");
    let exe_path = bin_dir.join(&exe_name);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::write(&exe_path, b"#!/bin/sh\necho hello\n").expect("write temp exe");
        let mut perms = fs::metadata(&exe_path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&exe_path, perms).expect("chmod +x");
    }
    #[cfg(windows)]
    {
        fs::write(&exe_path, b"@echo hello\r\n").expect("write temp exe");
    }

    let cmd_name = "test-tool";

    let custom_path = env::join_paths([&bin_dir]).expect("join paths");
    let found = which::which_in(cmd_name, Some(&custom_path), &work_dir)
        .expect("which must find executable in PATH");
    assert_eq!(found, exe_path);

    let empty_path = OsString::from("");
    let miss = which::which_in(cmd_name, Some(&empty_path), &work_dir);
    assert!(
        miss.is_err(),
        "which must report error when not on PATH and not in cwd"
    );

    let standard_lookup = which::which_in(cmd_name, Some(&empty_path), &work_dir).ok();
    let resolved = standard_lookup.or_else(|| {
        let candidates = vec![exe_path.clone()];
        candidates.into_iter().find(|p| is_executable_file(p))
    });
    assert_eq!(
        resolved,
        Some(exe_path.clone()),
        "fallback is triggered on lookup miss"
    );

    let resolved_direct = which::which_in(cmd_name, Some(&custom_path), &work_dir)
        .ok()
        .or_else(|| panic!("fallback should not be reached when standard lookup succeeds"));
    assert_eq!(resolved_direct, Some(exe_path));

    let _ = fs::remove_dir_all(&root_temp);
}

#[tokio::test(flavor = "current_thread")]
async fn streaming_stdout_line_listener_receives_lines_live() {
    let received = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let received_cb = received.clone();
    let listener = std::sync::Arc::new(move |line: &str| {
        received_cb.lock().unwrap().push(line.to_string());
    });

    let output = run_host_command_async_streaming(
        fixture_spec("lines-output", Duration::from_secs(5), 32 * 1024, 32 * 1024),
        None,
        Some(listener),
    )
    .await
    .expect("lines-output command should succeed");

    assert!(output.status.success());
    let lines: Vec<String> = received
        .lock()
        .unwrap()
        .iter()
        .filter(|l| l.starts_with("line-"))
        .cloned()
        .collect();
    assert_eq!(lines, vec!["line-1", "line-2", "line-3"]);
}

fn make_tokio_fixture_command(mode: &str) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(env::current_exe().expect("resolve test binary"));
    cmd.args([
        "--exact",
        "backend::host_process::tests::process_fixture",
        "--nocapture",
    ])
    .env("ASSETIWEAVE_HOST_PROCESS_FIXTURE", mode)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .stdin(Stdio::null());
    cmd
}
