use super::*;
use crate::backend::runtime::AppError;

fn invocation(entry: &str) -> ProcessInvocation {
    ProcessInvocation {
        kind: RuntimeProgramKind::Executable,
        entry: entry.to_string(),
        args: Vec::new(),
        env: Vec::new(),
        working_dir: None,
        version_req: None,
        immutable_install_dir: std::env::temp_dir(),
    }
}

async fn invoke_code(
    invocation: &ProcessInvocation,
    input_args: Vec<String>,
    limits: InvocationLimits,
    cancellation: CancellationToken,
) -> String {
    let mut invocation = invocation.clone();
    invocation.args = input_args;
    let error = ExtensionLauncher
        .invoke(&invocation, HostInput::Null, limits, cancellation)
        .await
        .expect_err("fixture must fail");
    AppError::from(error).view().code
}

#[tokio::test(flavor = "current_thread")]
async fn host_process_failures_keep_distinct_extension_codes() {
    let missing = invoke_code(
        &invocation("/tmp/assetiweave-missing-program"),
        Vec::new(),
        InvocationLimits {
            timeout: Duration::from_secs(1),
            stdout_limit: 64,
            stderr_limit: 64,
        },
        CancellationToken::new(),
    )
    .await;
    assert_eq!(missing, "program_not_found");

    let node_exe = crate::backend::host_process::resolve_host_executable("node")
        .unwrap_or_else(|| PathBuf::from("node"));
    let node_path = node_exe.to_string_lossy();

    let timeout = invoke_code(
        &invocation(&node_path),
        vec!["-e".into(), "setTimeout(() => {}, 2000)".into()],
        InvocationLimits {
            timeout: Duration::from_millis(50),
            stdout_limit: 64,
            stderr_limit: 64,
        },
        CancellationToken::new(),
    )
    .await;
    assert_eq!(timeout, "timeout");

    let cancelled = invoke_code(
        &invocation(&node_path),
        vec!["-e".into(), "setTimeout(() => {}, 2000)".into()],
        InvocationLimits {
            timeout: Duration::from_secs(1),
            stdout_limit: 64,
            stderr_limit: 64,
        },
        {
            let token = CancellationToken::new();
            token.cancel();
            token
        },
    )
    .await;
    assert_eq!(cancelled, "cancelled");

    let output_limit = invoke_code(
        &invocation(&node_path),
        vec!["-e".into(), "process.stdout.write('1234567890')".into()],
        InvocationLimits {
            timeout: Duration::from_secs(1),
            stdout_limit: 4,
            stderr_limit: 64,
        },
        CancellationToken::new(),
    )
    .await;
    assert_eq!(output_limit, "output_limit_exceeded");

    let nonzero = invoke_code(
        &invocation(&node_path),
        vec!["-e".into(), "process.exit(7)".into()],
        InvocationLimits {
            timeout: Duration::from_secs(1),
            stdout_limit: 64,
            stderr_limit: 64,
        },
        CancellationToken::new(),
    )
    .await;
    assert_eq!(nonzero, "nonzero_exit");
}

#[test]
fn launch_error_maps_spawn_and_cleanup_without_reclassifying_them() {
    let invocation = invocation("/tmp/private-agent");
    assert_eq!(
        AppError::from(launch_error(
            &invocation,
            HostProcessError::Spawn("permission denied".to_string()),
        ))
        .view()
        .code,
        "launch_failed"
    );
    assert_eq!(
        AppError::from(launch_error(
            &invocation,
            HostProcessError::Cleanup("cleanup failed".to_string()),
        ))
        .view()
        .code,
        "cleanup_failed"
    );
}
