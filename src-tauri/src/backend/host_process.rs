use std::{
    io::Read,
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub(crate) struct HostProcessOutput {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) stdout_truncated: bool,
    pub(crate) stderr_truncated: bool,
}

#[derive(Debug)]
pub(crate) enum HostProcessError {
    Spawn(String),
    Output(String),
    Timeout {
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        stdout_truncated: bool,
        stderr_truncated: bool,
    },
}

pub(crate) fn run_command_with_timeout(
    command: &mut Command,
    timeout: Duration,
    stdout_cap: usize,
    stderr_cap: usize,
) -> Result<HostProcessOutput, HostProcessError> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| HostProcessError::Spawn(error.to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| HostProcessError::Output("process stdout was not available".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| HostProcessError::Output("process stderr was not available".to_string()))?;
    let stdout_reader = thread::spawn(move || read_capped_and_drain(stdout, stdout_cap));
    let stderr_reader = thread::spawn(move || read_capped_and_drain(stderr, stderr_cap));
    let started = Instant::now();

    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| HostProcessError::Output(error.to_string()))?
        {
            let (stdout, stdout_truncated) = join_output_reader(stdout_reader, "stdout")?;
            let (stderr, stderr_truncated) = join_output_reader(stderr_reader, "stderr")?;
            return Ok(HostProcessOutput {
                status,
                stdout,
                stderr,
                stdout_truncated,
                stderr_truncated,
            });
        }

        if started.elapsed() >= timeout {
            terminate_child_tree(&mut child);
            let _ = child.wait();
            let (stdout, stdout_truncated) = join_output_reader(stdout_reader, "stdout")?;
            let (stderr, stderr_truncated) = join_output_reader(stderr_reader, "stderr")?;
            return Err(HostProcessError::Timeout {
                stdout,
                stderr,
                stdout_truncated,
                stderr_truncated,
            });
        }

        thread::sleep(Duration::from_millis(50));
    }
}

fn read_capped_and_drain<R: Read>(mut reader: R, cap: usize) -> Result<(Vec<u8>, bool), String> {
    let mut output = Vec::with_capacity(cap.min(8192));
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        let remaining = cap.saturating_sub(output.len());
        let retained = remaining.min(read);
        output.extend_from_slice(&buffer[..retained]);
        truncated |= retained < read;
    }
    Ok((output, truncated))
}

fn join_output_reader(
    reader: thread::JoinHandle<Result<(Vec<u8>, bool), String>>,
    stream: &str,
) -> Result<(Vec<u8>, bool), HostProcessError> {
    reader
        .join()
        .map_err(|_| HostProcessError::Output(format!("process {stream} reader panicked")))?
        .map_err(HostProcessError::Output)
}

#[cfg(windows)]
fn terminate_child_tree(child: &mut std::process::Child) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let status = Command::new("taskkill")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    if !status.is_ok_and(|status| status.success()) {
        let _ = child.kill();
    }
}

#[cfg(not(windows))]
fn terminate_child_tree(child: &mut std::process::Child) {
    let _ = child.kill();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env,
        io::{self, Write},
        process::Command,
        time::{Duration, Instant},
    };

    #[test]
    fn process_fixture() {
        match env::var("ASSETIWEAVE_HOST_PROCESS_FIXTURE").as_deref() {
            Ok("large-output") => {
                io::stdout().write_all(&vec![b'x'; 256 * 1024]).unwrap();
            }
            Ok("timeout") => std::thread::sleep(Duration::from_secs(5)),
            _ => {}
        }
    }

    #[test]
    fn drains_large_output_while_the_process_is_running() {
        let mut command = fixture_command("large-output");

        let output =
            run_command_with_timeout(&mut command, Duration::from_secs(5), 64 * 1024, 64 * 1024)
                .expect("large-output fixture should exit");

        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 64 * 1024);
        assert!(output.stdout_truncated);
    }

    #[test]
    fn terminates_and_reaps_processes_after_timeout() {
        let mut command = fixture_command("timeout");
        let started = Instant::now();

        let error = run_command_with_timeout(
            &mut command,
            Duration::from_millis(100),
            64 * 1024,
            64 * 1024,
        )
        .expect_err("timeout fixture should be terminated");

        assert!(matches!(error, HostProcessError::Timeout { .. }));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    fn fixture_command(mode: &str) -> Command {
        let mut command = Command::new(env::current_exe().expect("resolve test binary"));
        command
            .args([
                "--exact",
                "backend::host_process::tests::process_fixture",
                "--nocapture",
            ])
            .env("ASSETIWEAVE_HOST_PROCESS_FIXTURE", mode);
        command
    }
}
