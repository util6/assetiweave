use agent_client_protocol::schema::v1::SessionId;
use std::{fs, path::PathBuf};

use crate::infrastructure::host_process::{run_host_command, HostCommandSpec, HostInput};
use crate::infrastructure::process::managed_process as agent_process;
use crate::infrastructure::protocol::acp as acp_protocol;
use crate::{
    domain::{AgentDefinition, SESSION_ID_PLACEHOLDER},
    infrastructure::types::{
        AiExecutionRequest, AiExecutionSessionDeleteMethod, SessionCleanupStatus,
    },
};

const PROVIDER_SESSION_DELETE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

pub struct AcpExecutionGuard {
    pub workspace_root: Option<PathBuf>,
    pub workspace: PathBuf,
    pub process: Option<agent_process::ManagedAgentProcess>,
    pub protocol: Option<acp_protocol::AcpProtocol>,
    pub session_id: Option<SessionId>,
    pub cleaned: bool,
    pub bound_session: Option<String>,
    pub preserve_session: bool,
    pub preserve_workspace: bool,
}

impl AcpExecutionGuard {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace_root: None,
            workspace,
            process: None,
            protocol: None,
            session_id: None,
            cleaned: false,
            bound_session: None,
            preserve_session: false,
            preserve_workspace: false,
        }
    }

    pub fn with_root(workspace_root: PathBuf, workspace: PathBuf) -> Self {
        Self {
            workspace_root: Some(workspace_root),
            workspace,
            process: None,
            protocol: None,
            session_id: None,
            cleaned: false,
            bound_session: None,
            preserve_session: false,
            preserve_workspace: false,
        }
    }

    pub async fn cleanup(
        &mut self,
        cancel_before_close: bool,
        request: &AiExecutionRequest,
        definition: &AgentDefinition,
    ) -> CleanupReport {
        if self.cleaned {
            return CleanupReport::already_cleaned();
        }
        let mut report = CleanupReport::default();
        let mut standard_delete_failure = None;
        if self.session_id.is_some() {
            report.session_closed = Some(false);
            report.session_deleted = Some(false);
        }

        if let Some(protocol) = self.protocol.as_ref() {
            if cancel_before_close {
                if let Some(session_id) = self.session_id.clone() {
                    let cancel_timeout = request
                        .limits
                        .cancel_grace
                        .min(request.limits.close_timeout);
                    match protocol.cancel_and_wait(session_id, cancel_timeout).await {
                        Ok(()) => {}
                        Err(_) => report.failures.push("cancel".to_owned()),
                    }
                }
            }
            if let Some(session_id) = self.session_id.clone() {
                if self.preserve_session {
                    // Persistent keeps the provider session resumable; only
                    // the process is reaped below.
                } else {
                    match tokio::time::timeout(
                        request.limits.close_timeout,
                        protocol.close_session(session_id),
                    )
                    .await
                    {
                        Ok(Ok(Some(_))) => report.session_closed = Some(true),
                        Ok(Ok(None)) => report.session_closed = None,
                        Ok(Err(_)) => report.failures.push("close".to_owned()),
                        Err(_) => report.failures.push("close_timeout".to_owned()),
                    }
                }
            }
            if let Some(session_id) = self.session_id.clone() {
                if self.preserve_session {
                    // Persistent sessions must not be deleted.
                } else {
                    match tokio::time::timeout(
                        request.limits.close_timeout,
                        protocol.delete_session(session_id),
                    )
                    .await
                    {
                        Ok(Ok(Some(_))) => {
                            report.session_deleted = Some(true);
                            report.session_delete_method =
                                Some(AiExecutionSessionDeleteMethod::Acp);
                        }
                        Ok(Ok(None)) => standard_delete_failure = Some("delete_unsupported"),
                        Ok(Err(acp_protocol::AcpError::RequestFailed { message, .. }))
                            if matches_declared_not_found(
                                &message,
                                &definition.session_cleanup_not_found_markers,
                            ) =>
                        {
                            report.session_deleted = Some(true);
                            report.session_delete_method =
                                Some(AiExecutionSessionDeleteMethod::Acp);
                        }
                        Ok(Err(_)) => standard_delete_failure = Some("delete"),
                        Err(_) => standard_delete_failure = Some("delete_timeout"),
                    }
                }
            }
            if protocol
                .shutdown(request.limits.close_timeout)
                .await
                .is_err()
            {
                report.failures.push("protocol_shutdown".to_owned());
            }
        }
        self.protocol.take();

        if let Some(process) = self.process.as_ref() {
            report.process_id = Some(process.process_id());
            let termination = process.terminate(request.limits.cancel_grace).await;
            report.process_reaped = termination.exit.is_some();
            report.exit_code = termination.exit.as_ref().and_then(|exit| exit.code);
            if !termination.signal_errors.is_empty() {
                report.failures.push("process_signal".to_owned());
            }
            if !report.process_reaped {
                report.failures.push("process_reap".to_owned());
            }
            if !process
                .wait_for_stderr_eof(request.limits.close_timeout)
                .await
            {
                report.failures.push("stderr_join".to_owned());
            }
            match process.stderr_tail() {
                Ok(stderr) => {
                    report.stderr_bytes = stderr.bytes.len();
                    report.stderr_truncated = stderr.truncated;
                    if stderr.read_error {
                        report.failures.push("stderr_read".to_owned());
                    }
                }
                Err(_) => report.failures.push("stderr_state".to_owned()),
            }
        } else {
            report.process_reaped = true;
        }
        self.process.take();

        if !self.preserve_session
            && self.session_id.is_some()
            && report.session_deleted != Some(true)
        {
            let session_id = self
                .session_id
                .as_ref()
                .expect("session checked before fallback")
                .to_string();
            if let Some(cleanup) = definition.session_cleanup.as_ref() {
                let args = cleanup
                    .args
                    .iter()
                    .map(|arg| {
                        if arg == SESSION_ID_PLACEHOLDER {
                            session_id.clone()
                        } else {
                            arg.clone()
                        }
                    })
                    .collect();
                let output = run_host_command(
                    HostCommandSpec {
                        program: PathBuf::from(&definition.command),
                        args,
                        env: definition
                            .env
                            .iter()
                            .map(|entry| (entry.name.clone(), entry.value.clone()))
                            .collect(),
                        working_dir: Some(self.workspace.clone()),
                        stdin: HostInput::Null,
                        timeout: PROVIDER_SESSION_DELETE_TIMEOUT,
                        stdout_limit: request.limits.stderr_bytes,
                        stderr_limit: request.limits.stderr_bytes,
                    },
                    tokio_util::sync::CancellationToken::new(),
                )
                .await;
                match output {
                    Ok(output) if !output.stdout_truncated && !output.stderr_truncated => {
                        let missing_is_success = !output.status.success()
                            && std::str::from_utf8(&output.stderr)
                                .ok()
                                .is_some_and(|stderr| {
                                    matches_declared_not_found(
                                        stderr,
                                        &definition.session_cleanup_not_found_markers,
                                    )
                                });
                        if output.status.success() || missing_is_success {
                            report.session_deleted = Some(true);
                            report.session_delete_method =
                                Some(AiExecutionSessionDeleteMethod::ProviderFallback);
                        } else {
                            if let Some(failure) = standard_delete_failure {
                                report.failures.push(failure.to_owned());
                            }
                            report.failures.push("delete_fallback".to_owned());
                        }
                    }
                    _ => {
                        if let Some(failure) = standard_delete_failure {
                            report.failures.push(failure.to_owned());
                        }
                        report.failures.push("delete_fallback".to_owned());
                    }
                }
            } else if let Some(failure) = standard_delete_failure {
                report.failures.push(failure.to_owned());
            }
        }

        report.workspace_removed = if self.preserve_workspace {
            false
        } else {
            let removed = match fs::remove_dir_all(&self.workspace) {
                Ok(()) => true,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
                Err(_) => {
                    report.failures.push("workspace_remove".to_owned());
                    false
                }
            };
            if removed {
                if let (Some(root), Some(parent)) =
                    (self.workspace_root.as_ref(), self.workspace.parent())
                {
                    if parent != root {
                        let _ = fs::remove_dir(parent);
                    }
                }
            }
            removed
        };
        self.cleaned = true;
        report
    }
}

pub fn matches_declared_not_found(message: &str, markers: &[String]) -> bool {
    message.lines().any(|line| {
        let line = line.trim_start();
        markers.iter().any(|marker| line.starts_with(marker))
    })
}

#[derive(Default, Debug)]
pub struct CleanupReport {
    pub failures: Vec<String>,
    pub process_id: Option<u32>,
    pub process_reaped: bool,
    pub workspace_removed: bool,
    pub stderr_bytes: usize,
    pub stderr_truncated: bool,
    pub exit_code: Option<i32>,
    pub session_deleted: Option<bool>,
    pub session_closed: Option<bool>,
    pub session_delete_method: Option<AiExecutionSessionDeleteMethod>,
}

impl CleanupReport {
    pub fn already_cleaned() -> Self {
        Self {
            process_reaped: true,
            workspace_removed: true,
            ..Self::default()
        }
    }

    pub fn timed_out() -> Self {
        Self {
            failures: vec!["cleanup_timeout".to_owned()],
            ..Self::default()
        }
    }
}

pub fn is_session_delete_failure(failure: &str) -> bool {
    matches!(
        failure,
        "delete_unsupported" | "delete" | "delete_timeout" | "delete_fallback"
    )
}

pub fn determine_session_cleanup_status(cleanup: &CleanupReport) -> SessionCleanupStatus {
    if cleanup.session_deleted == Some(true) {
        return SessionCleanupStatus::Deleted;
    }
    let delete_failures: Vec<&str> = cleanup
        .failures
        .iter()
        .filter(|f| matches!(f.as_str(), "delete_fallback" | "delete" | "delete_timeout"))
        .map(String::as_str)
        .collect();
    if !delete_failures.is_empty() {
        return SessionCleanupStatus::Failed(delete_failures.join(", "));
    }
    if cleanup.failures.iter().any(|f| f == "delete_unsupported") {
        return SessionCleanupStatus::Unsupported;
    }
    if cleanup.session_deleted.is_none() {
        SessionCleanupStatus::Skipped
    } else {
        SessionCleanupStatus::Failed("session_delete_unsuccessful".to_string())
    }
}

pub fn is_transient_probe_failure(code: &str) -> bool {
    let lower = code.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "cleanup_failed"
            | "connection_failed"
            | "agent_exited"
            | "timeout"
            | "initialize_timeout"
            | "session_new_timeout"
            | "config_timeout"
            | "cleanup_timeout"
            | "disconnect"
            | "transport_error"
            | "stdio_unavailable"
            | "take_stdio"
    ) || lower.ends_with("_timeout")
        || lower.contains("timeout")
}

#[cfg(test)]
#[path = "acp_guard_tests.rs"]
mod tests;
