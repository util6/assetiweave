use std::{
    fmt,
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
            ClientCapabilities, ContentBlock, Implementation, InitializeRequest,
            InitializeResponse, RequestPermissionOutcome, RequestPermissionRequest,
            RequestPermissionResponse, SessionId, SessionNotification, SessionUpdate, StopReason,
            ToolCall, ToolCallStatus, ToolCallUpdate,
        },
        ProtocolVersion,
    },
    Agent, Client, ConnectTo, ConnectionTo,
};
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};

const ACP_CLIENT_NAME: &str = "AssetIWeave";
const ACP_CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

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

pub(crate) fn request_failed(
    operation: AcpOperation,
    error: agent_client_protocol::Error,
) -> AcpError {
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

pub(crate) fn build_initialize_request() -> InitializeRequest {
    InitializeRequest::new(ProtocolVersion::V1)
        .client_info(Implementation::new(ACP_CLIENT_NAME, ACP_CLIENT_VERSION))
        .client_capabilities(ClientCapabilities::default())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_sdk_actor(
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

pub(crate) fn normalize_session_notification(notification: SessionNotification) -> AcpRuntimeEvent {
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

pub(crate) fn take_mutex_option<T>(
    mutex: &Mutex<Option<T>>,
    state: &'static str,
) -> Result<Option<T>, AcpError> {
    mutex
        .lock()
        .map_err(|_| AcpError::StateUnavailable(state))
        .map(|mut value| value.take())
}

pub(crate) async fn abort_and_join(actor: JoinHandle<()>) {
    actor.abort();
    let _ = actor.await;
}
