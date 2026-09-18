use std::{
    fmt,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use agent_client_protocol::{
    on_receive_notification, on_receive_request,
    schema::{
        v1::{
            CancelNotification, ClientCapabilities, CloseSessionRequest, CloseSessionResponse,
            ContentBlock, DeleteSessionRequest, DeleteSessionResponse, Implementation,
            InitializeRequest, InitializeResponse, LoadSessionRequest, LoadSessionResponse,
            McpServer, NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse,
            RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
            ResumeSessionRequest, ResumeSessionResponse, SessionId, SessionNotification,
            SessionUpdate, SetSessionConfigOptionRequest, SetSessionConfigOptionResponse,
            StopReason, TextContent, ToolCall, ToolCallStatus, ToolCallUpdate,
        },
        ProtocolVersion,
    },
    Agent, ByteStreams, Client, ConnectTo, ConnectionTo,
};
use tokio::{
    process::{ChildStdin, ChildStdout},
    sync::{mpsc, oneshot, watch},
    task::{AbortHandle, JoinHandle},
};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

const ACP_CLIENT_NAME: &str = "AssetIWeave";
const ACP_CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) struct AcpProtocol {
    connection: ConnectionTo<Agent>,
    initialize: InitializeResponse,
    event_tx: mpsc::Sender<AcpRuntimeEvent>,
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    actor: Mutex<Option<JoinHandle<()>>>,
    actor_abort: AbortHandle,
    #[cfg(test)]
    alive: Arc<AtomicBool>,
    shutdown_requested: Arc<AtomicBool>,
}

impl AcpProtocol {
    pub(crate) async fn connect(
        stdin: ChildStdin,
        stdout: ChildStdout,
        config: AcpConnectConfig,
    ) -> Result<(Self, AcpProtocolChannels), AcpError> {
        let transport = ByteStreams::new(stdin.compat_write(), stdout.compat());
        Self::connect_transport(transport, config).await
    }

    async fn connect_transport(
        transport: impl ConnectTo<Client> + 'static,
        config: AcpConnectConfig,
    ) -> Result<(Self, AcpProtocolChannels), AcpError> {
        let (event_tx, events) = mpsc::channel(config.event_channel_capacity.max(1));
        let (disconnect_tx, disconnects) = watch::channel(None);
        let (initialize_tx, initialize_rx) = oneshot::channel();
        let (ready_tx, ready_rx) = oneshot::channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let alive = Arc::new(AtomicBool::new(true));
        let shutdown_requested = Arc::new(AtomicBool::new(false));

        let actor = tokio::spawn(run_sdk_actor(
            transport,
            event_tx.clone(),
            disconnect_tx,
            initialize_tx,
            ready_tx,
            shutdown_rx,
            Arc::clone(&alive),
            Arc::clone(&shutdown_requested),
        ));
        let actor_abort = actor.abort_handle();

        let initialize = match tokio::time::timeout(config.initialize_timeout, initialize_rx).await
        {
            Ok(Ok(Ok(initialize))) => initialize,
            Ok(Ok(Err(error))) => {
                abort_and_join(actor).await;
                return Err(error);
            }
            Ok(Err(_)) => {
                abort_and_join(actor).await;
                return Err(AcpError::ActorClosedDuringInitialize);
            }
            Err(_) => {
                abort_and_join(actor).await;
                return Err(AcpError::InitializeTimeout {
                    timeout: config.initialize_timeout,
                });
            }
        };
        let connection = match ready_rx.await {
            Ok(connection) => connection,
            Err(_) => {
                abort_and_join(actor).await;
                return Err(AcpError::ActorClosedDuringInitialize);
            }
        };

        Ok((
            Self {
                connection,
                initialize,
                event_tx,
                shutdown_tx: Mutex::new(Some(shutdown_tx)),
                actor: Mutex::new(Some(actor)),
                actor_abort,
                #[cfg(test)]
                alive,
                shutdown_requested,
            },
            AcpProtocolChannels {
                events,
                disconnects,
            },
        ))
    }

    #[cfg(test)]
    pub(crate) fn initialize_response(&self) -> &InitializeResponse {
        &self.initialize
    }

    #[cfg(test)]
    pub(crate) fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    pub(crate) async fn new_session(&self, cwd: PathBuf) -> Result<NewSessionResponse, AcpError> {
        self.new_session_with_mcp(cwd, Vec::new()).await
    }

    pub(crate) async fn new_session_with_mcp(
        &self,
        cwd: PathBuf,
        mcp_servers: Vec<McpServer>,
    ) -> Result<NewSessionResponse, AcpError> {
        self.connection
            .send_request(NewSessionRequest::new(cwd).mcp_servers(mcp_servers))
            .block_task()
            .await
            .map_err(|error| request_failed(AcpOperation::NewSession, error))
    }

    pub(crate) fn supports_load(&self) -> bool {
        self.initialize.agent_capabilities.load_session
    }

    pub(crate) fn supports_resume(&self) -> bool {
        self.initialize
            .agent_capabilities
            .session_capabilities
            .resume
            .is_some()
    }

    pub(crate) async fn load_session(
        &self,
        session_id: SessionId,
        cwd: PathBuf,
    ) -> Result<LoadSessionResponse, AcpError> {
        self.load_session_with_mcp(session_id, cwd, Vec::new())
            .await
    }

    pub(crate) async fn load_session_with_mcp(
        &self,
        session_id: SessionId,
        cwd: PathBuf,
        mcp_servers: Vec<McpServer>,
    ) -> Result<LoadSessionResponse, AcpError> {
        self.connection
            .send_request(LoadSessionRequest::new(session_id, cwd).mcp_servers(mcp_servers))
            .block_task()
            .await
            .map_err(|error| request_failed(AcpOperation::LoadSession, error))
    }

    pub(crate) async fn resume_session(
        &self,
        session_id: SessionId,
        cwd: PathBuf,
    ) -> Result<ResumeSessionResponse, AcpError> {
        self.resume_session_with_mcp(session_id, cwd, Vec::new())
            .await
    }

    pub(crate) async fn resume_session_with_mcp(
        &self,
        session_id: SessionId,
        cwd: PathBuf,
        mcp_servers: Vec<McpServer>,
    ) -> Result<ResumeSessionResponse, AcpError> {
        self.connection
            .send_request(ResumeSessionRequest::new(session_id, cwd).mcp_servers(mcp_servers))
            .block_task()
            .await
            .map_err(|error| request_failed(AcpOperation::ResumeSession, error))
    }

    pub(crate) async fn set_model(
        &self,
        session_id: SessionId,
        model: &str,
        timeout: Duration,
    ) -> Result<SetSessionConfigOptionResponse, AcpError> {
        let request = SetSessionConfigOptionRequest::new(session_id, "model", model);
        tokio::time::timeout(timeout, self.connection.send_request(request).block_task())
            .await
            .map_err(|_| AcpError::RequestTimeout {
                operation: AcpOperation::SetModel,
                timeout,
            })?
            .map_err(|error| request_failed(AcpOperation::SetModel, error))
    }

    pub(crate) async fn prompt(
        &self,
        session_id: SessionId,
        prompt: String,
    ) -> Result<PromptResponse, AcpError> {
        let completion_session_id = session_id.clone();
        let response = self
            .connection
            .send_request(PromptRequest::new(
                session_id,
                vec![ContentBlock::Text(TextContent::new(prompt))],
            ))
            .block_task()
            .await
            .map_err(|error| request_failed(AcpOperation::Prompt, error))?;
        self.event_tx
            .send(AcpRuntimeEvent::TurnCompleted {
                session_id: completion_session_id,
                stop_reason: response.stop_reason,
            })
            .await
            .map_err(|_| AcpError::RequestFailed {
                operation: AcpOperation::Prompt,
                message: "the local ACP event stream closed".to_string(),
            })?;
        Ok(response)
    }

    pub(crate) fn cancel(&self, session_id: SessionId) -> Result<(), AcpError> {
        self.connection
            .send_notification(CancelNotification::new(session_id))
            .map_err(|_| AcpError::RequestFailed {
                operation: AcpOperation::Cancel,
                message: "the ACP cancellation notification could not be sent".to_string(),
            })
    }

    /// Send cancellation while the transport is still writable and yield to
    /// the SDK actor once so the notification write is observed before the
    /// caller starts closing the session/process. The SDK notification API has
    /// no response/flush future, so this is the bounded transport boundary we
    /// can enforce without waiting indefinitely on a non-cooperative agent.
    pub(crate) async fn cancel_and_wait(
        &self,
        session_id: SessionId,
        timeout: Duration,
    ) -> Result<(), AcpError> {
        self.cancel(session_id)?;
        tokio::time::timeout(timeout, tokio::task::yield_now())
            .await
            .map_err(|_| AcpError::RequestTimeout {
                operation: AcpOperation::Cancel,
                timeout,
            })?;
        Ok(())
    }

    pub(crate) async fn close_session(
        &self,
        session_id: SessionId,
    ) -> Result<Option<CloseSessionResponse>, AcpError> {
        if self
            .initialize
            .agent_capabilities
            .session_capabilities
            .close
            .is_none()
        {
            return Ok(None);
        }
        self.connection
            .send_request(CloseSessionRequest::new(session_id))
            .block_task()
            .await
            .map(Some)
            .map_err(|error| request_failed(AcpOperation::CloseSession, error))
    }

    pub(crate) async fn delete_session(
        &self,
        session_id: SessionId,
    ) -> Result<Option<DeleteSessionResponse>, AcpError> {
        if self
            .initialize
            .agent_capabilities
            .session_capabilities
            .delete
            .is_none()
        {
            return Ok(None);
        }
        self.connection
            .send_request(DeleteSessionRequest::new(session_id))
            .block_task()
            .await
            .map(Some)
            .map_err(|error| request_failed(AcpOperation::DeleteSession, error))
    }

    pub(crate) async fn shutdown(&self, timeout: Duration) -> Result<(), AcpError> {
        self.shutdown_requested.store(true, Ordering::Release);
        if let Some(shutdown_tx) = take_mutex_option(&self.shutdown_tx, "shutdown")? {
            let _ = shutdown_tx.send(());
        }
        let Some(actor) = take_mutex_option(&self.actor, "actor")? else {
            return Ok(());
        };
        match tokio::time::timeout(timeout, actor).await {
            Ok(_) => Ok(()),
            Err(_) => {
                self.actor_abort.abort();
                Err(AcpError::ShutdownTimeout { timeout })
            }
        }
    }
}

impl Drop for AcpProtocol {
    fn drop(&mut self) {
        self.shutdown_requested.store(true, Ordering::Release);
        if let Ok(shutdown_tx) = self.shutdown_tx.get_mut() {
            if let Some(shutdown_tx) = shutdown_tx.take() {
                let _ = shutdown_tx.send(());
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AcpConnectConfig {
    pub(crate) initialize_timeout: Duration,
    pub(crate) event_channel_capacity: usize,
}

impl AcpConnectConfig {
    pub(crate) fn new(initialize_timeout: Duration) -> Self {
        Self {
            initialize_timeout,
            event_channel_capacity: 64,
        }
    }
}

pub(crate) struct AcpProtocolChannels {
    pub(crate) events: mpsc::Receiver<AcpRuntimeEvent>,
    pub(crate) disconnects: watch::Receiver<Option<AcpDisconnect>>,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) enum AcpRuntimeEvent {
    AgentText {
        session_id: SessionId,
        text: String,
    },
    AgentThought {
        session_id: SessionId,
        text: Option<String>,
    },
    ToolCall {
        session_id: SessionId,
        tool_call_id: String,
        title: String,
        status: AcpToolStatus,
        raw_input: Option<serde_json::Value>,
        raw_output: Option<serde_json::Value>,
    },
    ToolCallUpdate {
        session_id: SessionId,
        tool_call_id: String,
        title: Option<String>,
        status: Option<AcpToolStatus>,
        raw_input: Option<serde_json::Value>,
        raw_output: Option<serde_json::Value>,
    },
    PermissionRequested {
        session_id: SessionId,
    },
    Other {
        session_id: SessionId,
    },
    TurnCompleted {
        session_id: SessionId,
        stop_reason: StopReason,
    },
}

impl fmt::Debug for AcpRuntimeEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AgentText { session_id, .. } => formatter
                .debug_struct("AgentText")
                .field("session_id", session_id)
                .field("text", &"<redacted>")
                .finish(),
            Self::AgentThought { session_id, text } => formatter
                .debug_struct("AgentThought")
                .field("session_id", session_id)
                .field("text", &text.as_ref().map(|_| "<redacted>"))
                .finish(),
            Self::ToolCall {
                session_id,
                tool_call_id,
                title,
                status,
                raw_input,
                raw_output,
            } => formatter
                .debug_struct("ToolCall")
                .field("session_id", session_id)
                .field("tool_call_id", tool_call_id)
                .field("title", title)
                .field("status", status)
                .field("raw_input", &raw_input.as_ref().map(|_| "<redacted>"))
                .field("raw_output", &raw_output.as_ref().map(|_| "<redacted>"))
                .finish(),
            Self::ToolCallUpdate {
                session_id,
                tool_call_id,
                title,
                status,
                raw_input,
                raw_output,
            } => formatter
                .debug_struct("ToolCallUpdate")
                .field("session_id", session_id)
                .field("tool_call_id", tool_call_id)
                .field("title", title)
                .field("status", status)
                .field("raw_input", &raw_input.as_ref().map(|_| "<redacted>"))
                .field("raw_output", &raw_output.as_ref().map(|_| "<redacted>"))
                .finish(),
            Self::PermissionRequested { session_id } => formatter
                .debug_struct("PermissionRequested")
                .field("session_id", session_id)
                .finish(),
            Self::Other { session_id } => formatter
                .debug_struct("Other")
                .field("session_id", session_id)
                .finish(),
            Self::TurnCompleted {
                session_id,
                stop_reason,
            } => formatter
                .debug_struct("TurnCompleted")
                .field("session_id", session_id)
                .field("stop_reason", stop_reason)
                .finish(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AcpToolStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

impl From<ToolCallStatus> for AcpToolStatus {
    fn from(status: ToolCallStatus) -> Self {
        match status {
            ToolCallStatus::Pending => Self::Pending,
            ToolCallStatus::InProgress => Self::InProgress,
            ToolCallStatus::Completed => Self::Completed,
            ToolCallStatus::Failed => Self::Failed,
            _ => Self::Pending,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AcpDisconnect {
    pub(crate) reason: AcpDisconnectReason,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AcpDisconnectReason {
    Shutdown,
    TransportClosed,
    ProtocolError,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AcpOperation {
    NewSession,
    LoadSession,
    ResumeSession,
    SetModel,
    Prompt,
    Cancel,
    CloseSession,
    DeleteSession,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum AcpError {
    #[error("ACP initialize timed out after {} milliseconds", timeout.as_millis())]
    InitializeTimeout { timeout: Duration },
    #[error("ACP initialize failed")]
    InitializeFailed,
    #[error("ACP connection closed during initialize")]
    ActorClosedDuringInitialize,
    #[error("ACP {operation:?} timed out after {} milliseconds", timeout.as_millis())]
    RequestTimeout {
        operation: AcpOperation,
        timeout: Duration,
    },
    #[error("ACP {operation:?} failed: {message}")]
    RequestFailed {
        operation: AcpOperation,
        message: String,
    },
    #[error("ACP {0} state is unavailable")]
    StateUnavailable(&'static str),
    #[error("ACP shutdown timed out after {} milliseconds", timeout.as_millis())]
    ShutdownTimeout { timeout: Duration },
}

fn request_failed(operation: AcpOperation, error: agent_client_protocol::Error) -> AcpError {
    let summary = sanitize_protocol_error_message(&error.message);
    let details = error
        .data
        .as_ref()
        .and_then(|data| data.get("details"))
        .and_then(serde_json::Value::as_str)
        .map(sanitize_protocol_error_message)
        .filter(|details| details != &summary);
    AcpError::RequestFailed {
        operation,
        message: details
            .map(|details| format!("{summary}: {details}"))
            .unwrap_or(summary),
    }
}

fn sanitize_protocol_error_message(message: &str) -> String {
    let normalized = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return "the Agent returned an empty protocol error".to_string();
    }
    normalized.chars().take(500).collect()
}

fn build_initialize_request() -> InitializeRequest {
    InitializeRequest::new(ProtocolVersion::V1)
        .client_info(Implementation::new(ACP_CLIENT_NAME, ACP_CLIENT_VERSION))
        .client_capabilities(ClientCapabilities::default())
}

#[allow(clippy::too_many_arguments)]
async fn run_sdk_actor(
    transport: impl ConnectTo<Client> + 'static,
    event_tx: mpsc::Sender<AcpRuntimeEvent>,
    disconnect_tx: watch::Sender<Option<AcpDisconnect>>,
    initialize_tx: oneshot::Sender<Result<InitializeResponse, AcpError>>,
    ready_tx: oneshot::Sender<ConnectionTo<Agent>>,
    shutdown_rx: oneshot::Receiver<()>,
    alive: Arc<AtomicBool>,
    shutdown_requested: Arc<AtomicBool>,
) {
    let mut initialize_tx = Some(initialize_tx);
    let mut ready_tx = Some(ready_tx);
    let mut shutdown_rx = Some(shutdown_rx);

    let result = Client
        .builder()
        .on_receive_notification(
            {
                let event_tx = event_tx.clone();
                async move |notification: SessionNotification, _connection| {
                    let event = normalize_session_notification(notification);
                    let _ = event_tx.send(event).await;
                    Ok(())
                }
            },
            on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, _connection| {
                let _ = event_tx
                    .send(AcpRuntimeEvent::PermissionRequested {
                        session_id: request.session_id,
                    })
                    .await;
                responder.respond(RequestPermissionResponse::new(
                    RequestPermissionOutcome::Cancelled,
                ))
            },
            on_receive_request!(),
        )
        .connect_with(transport, async move |connection: ConnectionTo<Agent>| {
            let initialize = connection
                .send_request(build_initialize_request())
                .block_task()
                .await;
            let Some(initialize_tx) = initialize_tx.take() else {
                return Ok(());
            };
            match initialize {
                Ok(initialize) => {
                    let _ = initialize_tx.send(Ok(initialize));
                }
                Err(_) => {
                    let _ = initialize_tx.send(Err(AcpError::InitializeFailed));
                    return Ok(());
                }
            }
            if let Some(ready_tx) = ready_tx.take() {
                if ready_tx.send(connection).is_err() {
                    return Ok(());
                }
            }
            if let Some(shutdown_rx) = shutdown_rx.take() {
                let _ = shutdown_rx.await;
            }
            Ok(())
        })
        .await;

    alive.store(false, Ordering::Release);
    let reason = if shutdown_requested.load(Ordering::Acquire) {
        AcpDisconnectReason::Shutdown
    } else if result.is_ok() {
        AcpDisconnectReason::TransportClosed
    } else {
        AcpDisconnectReason::ProtocolError
    };
    let _ = disconnect_tx.send(Some(AcpDisconnect { reason }));
}

fn normalize_session_notification(notification: SessionNotification) -> AcpRuntimeEvent {
    let session_id = notification.session_id;
    match notification.update {
        SessionUpdate::AgentMessageChunk(chunk) => match chunk.content {
            ContentBlock::Text(text) => AcpRuntimeEvent::AgentText {
                session_id,
                text: text.text,
            },
            _ => AcpRuntimeEvent::Other { session_id },
        },
        SessionUpdate::AgentThoughtChunk(chunk) => match chunk.content {
            ContentBlock::Text(text) => AcpRuntimeEvent::AgentThought {
                session_id,
                text: Some(text.text),
            },
            _ => AcpRuntimeEvent::AgentThought {
                session_id,
                text: None,
            },
        },
        SessionUpdate::ToolCall(ToolCall {
            tool_call_id,
            title,
            status,
            raw_input,
            raw_output,
            ..
        }) => AcpRuntimeEvent::ToolCall {
            session_id,
            tool_call_id: tool_call_id.to_string(),
            title,
            status: status.into(),
            raw_input,
            raw_output,
        },
        SessionUpdate::ToolCallUpdate(ToolCallUpdate {
            tool_call_id,
            fields,
            ..
        }) => AcpRuntimeEvent::ToolCallUpdate {
            session_id,
            tool_call_id: tool_call_id.to_string(),
            title: fields.title,
            status: fields.status.map(Into::into),
            raw_input: fields.raw_input,
            raw_output: fields.raw_output,
        },
        _ => AcpRuntimeEvent::Other { session_id },
    }
}

fn take_mutex_option<T>(
    mutex: &Mutex<Option<T>>,
    state: &'static str,
) -> Result<Option<T>, AcpError> {
    mutex
        .lock()
        .map_err(|_| AcpError::StateUnavailable(state))
        .map(|mut value| value.take())
}

async fn abort_and_join(actor: JoinHandle<()>) {
    actor.abort();
    let _ = actor.await;
}

#[cfg(test)]
#[path = "acp_tests.rs"]
mod tests;
