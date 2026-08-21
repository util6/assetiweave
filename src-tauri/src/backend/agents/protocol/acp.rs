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
            ContentBlock, Implementation, InitializeRequest, InitializeResponse, NewSessionRequest,
            NewSessionResponse, PromptRequest, PromptResponse, RequestPermissionOutcome,
            RequestPermissionRequest, RequestPermissionResponse, SessionId, SessionNotification,
            SessionUpdate, SetSessionConfigOptionRequest, SetSessionConfigOptionResponse,
            StopReason, TextContent,
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
        self.connection
            .send_request(NewSessionRequest::new(cwd))
            .block_task()
            .await
            .map_err(|_| AcpError::RequestFailed {
                operation: AcpOperation::NewSession,
            })
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
            .map_err(|_| AcpError::RequestFailed {
                operation: AcpOperation::SetModel,
            })
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
            .map_err(|_| AcpError::RequestFailed {
                operation: AcpOperation::Prompt,
            })?;
        self.event_tx
            .send(AcpRuntimeEvent::TurnCompleted {
                session_id: completion_session_id,
                stop_reason: response.stop_reason,
            })
            .await
            .map_err(|_| AcpError::RequestFailed {
                operation: AcpOperation::Prompt,
            })?;
        Ok(response)
    }

    pub(crate) fn cancel(&self, session_id: SessionId) -> Result<(), AcpError> {
        self.connection
            .send_notification(CancelNotification::new(session_id))
            .map_err(|_| AcpError::RequestFailed {
                operation: AcpOperation::Cancel,
            })
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
            .map_err(|_| AcpError::RequestFailed {
                operation: AcpOperation::CloseSession,
            })
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AcpRuntimeEvent {
    AgentText {
        session_id: SessionId,
        text: String,
    },
    AgentThought {
        session_id: SessionId,
    },
    ToolActivity {
        session_id: SessionId,
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
    SetModel,
    Prompt,
    Cancel,
    CloseSession,
}

#[derive(Debug)]
pub(crate) enum AcpError {
    InitializeTimeout {
        timeout: Duration,
    },
    InitializeFailed,
    ActorClosedDuringInitialize,
    RequestTimeout {
        operation: AcpOperation,
        timeout: Duration,
    },
    RequestFailed {
        operation: AcpOperation,
    },
    StateUnavailable(&'static str),
    ShutdownTimeout {
        timeout: Duration,
    },
}

impl fmt::Display for AcpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InitializeTimeout { timeout } => write!(
                formatter,
                "ACP initialize timed out after {} milliseconds",
                timeout.as_millis()
            ),
            Self::InitializeFailed => formatter.write_str("ACP initialize failed"),
            Self::ActorClosedDuringInitialize => {
                formatter.write_str("ACP connection closed during initialize")
            }
            Self::RequestTimeout { operation, timeout } => write!(
                formatter,
                "ACP {operation:?} timed out after {} milliseconds",
                timeout.as_millis()
            ),
            Self::RequestFailed { operation } => write!(formatter, "ACP {operation:?} failed"),
            Self::StateUnavailable(state) => write!(formatter, "ACP {state} state is unavailable"),
            Self::ShutdownTimeout { timeout } => write!(
                formatter,
                "ACP shutdown timed out after {} milliseconds",
                timeout.as_millis()
            ),
        }
    }
}

impl std::error::Error for AcpError {}

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
        SessionUpdate::AgentThoughtChunk(_) => AcpRuntimeEvent::AgentThought { session_id },
        SessionUpdate::ToolCall(_) | SessionUpdate::ToolCallUpdate(_) => {
            AcpRuntimeEvent::ToolActivity { session_id }
        }
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
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{
        AgentCapabilities, CloseSessionRequest, NewSessionRequest, PermissionOption,
        PermissionOptionKind, PromptRequest, SessionCapabilities, SessionCloseCapabilities,
        SetSessionConfigOptionRequest, ToolCall, ToolCallUpdate, ToolCallUpdateFields,
    };
    use agent_client_protocol::{on_receive_notification, on_receive_request, Channel};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn initialize_request_has_identity_and_minimal_client_capabilities() {
        let request = build_initialize_request();
        let client_info = request.client_info.expect("client info");

        assert!(!client_info.name.is_empty());
        assert!(!client_info.version.is_empty());
        assert!(!request.client_capabilities.terminal);
        assert!(request.client_capabilities.session.is_none());
        assert!(!request.client_capabilities.fs.read_text_file);
        assert!(!request.client_capabilities.fs.write_text_file);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn connect_caches_initialize_and_shutdown_is_idempotent() {
        let transport = initialized_agent_transport(InitializeResponse::new(ProtocolVersion::V1));
        let (protocol, mut channels) = AcpProtocol::connect_transport(
            transport,
            AcpConnectConfig::new(Duration::from_millis(250)),
        )
        .await
        .expect("connect protocol");

        assert_eq!(
            protocol.initialize_response().protocol_version,
            ProtocolVersion::V1
        );
        assert!(protocol.is_alive());
        protocol
            .shutdown(Duration::from_millis(250))
            .await
            .expect("first shutdown");
        protocol
            .shutdown(Duration::from_millis(250))
            .await
            .expect("repeated shutdown");
        channels
            .disconnects
            .changed()
            .await
            .expect("disconnect update");
        assert_eq!(
            channels.disconnects.borrow().as_ref().unwrap().reason,
            AcpDisconnectReason::Shutdown
        );
        assert!(!protocol.is_alive());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn initialize_has_a_local_configurable_timeout() {
        let (client_transport, agent_transport) = Channel::duplex();
        tokio::spawn(async move {
            let _ = Agent
                .builder()
                .on_receive_request(
                    async |_request: InitializeRequest, _responder, _connection| {
                        std::future::pending::<Result<(), agent_client_protocol::Error>>().await
                    },
                    on_receive_request!(),
                )
                .connect_to(agent_transport)
                .await;
        });

        let result = AcpProtocol::connect_transport(
            client_transport,
            AcpConnectConfig::new(Duration::from_millis(50)),
        )
        .await;
        let error = match result {
            Ok(_) => panic!("initialize unexpectedly completed"),
            Err(error) => error,
        };

        assert!(matches!(error, AcpError::InitializeTimeout { .. }));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn close_session_is_gated_by_advertised_capability() {
        let transport = initialized_agent_transport(InitializeResponse::new(ProtocolVersion::V1));
        let (protocol, _channels) = AcpProtocol::connect_transport(
            transport,
            AcpConnectConfig::new(Duration::from_millis(250)),
        )
        .await
        .expect("connect protocol");

        let result = protocol
            .close_session(SessionId::new("session"))
            .await
            .expect("capability skip");

        assert!(result.is_none());
        protocol.shutdown(Duration::from_millis(250)).await.unwrap();
    }

    #[derive(Default)]
    struct OperationObservations {
        cwd: Mutex<Option<PathBuf>>,
        model: Mutex<Option<(SessionId, String, String)>>,
        prompt: Mutex<Option<(SessionId, Vec<ContentBlock>)>>,
        cancel_session: Mutex<Option<SessionId>>,
        close_session: Mutex<Option<SessionId>>,
    }

    #[tokio::test(flavor = "current_thread")]
    async fn phase_one_operations_use_typed_requests_and_cancel_in_flight_prompt() {
        let observations = Arc::new(OperationObservations::default());
        let cancel = Arc::new(tokio::sync::Notify::new());
        let (client_transport, agent_transport) = Channel::duplex();
        let agent_observations = Arc::clone(&observations);
        let agent_cancel = Arc::clone(&cancel);
        tokio::spawn(async move {
            let initialize = initialize_with_close_capability();
            let new_observations = Arc::clone(&agent_observations);
            let model_observations = Arc::clone(&agent_observations);
            let prompt_observations = Arc::clone(&agent_observations);
            let cancel_observations = Arc::clone(&agent_observations);
            let close_observations = Arc::clone(&agent_observations);
            let prompt_cancel = Arc::clone(&agent_cancel);
            let notification_cancel = Arc::clone(&agent_cancel);
            let _ = Agent
                .builder()
                .on_receive_request(
                    async move |_request: InitializeRequest, responder, _connection| {
                        responder.respond(initialize.clone())
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: NewSessionRequest, responder, _connection| {
                        *new_observations.cwd.lock().unwrap() = Some(request.cwd);
                        assert!(request.additional_directories.is_empty());
                        assert!(request.mcp_servers.is_empty());
                        responder.respond(NewSessionResponse::new("session-typed"))
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: SetSessionConfigOptionRequest, responder, _connection| {
                        *model_observations.model.lock().unwrap() = Some((
                            request.session_id,
                            request.config_id.to_string(),
                            request
                                .value
                                .as_value_id()
                                .expect("select model value")
                                .to_string(),
                        ));
                        responder.respond(SetSessionConfigOptionResponse::new(Vec::new()))
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: PromptRequest, responder, connection| {
                        let observations = Arc::clone(&prompt_observations);
                        let cancel = Arc::clone(&prompt_cancel);
                        *observations.prompt.lock().unwrap() =
                            Some((request.session_id, request.prompt));
                        connection.spawn(async move {
                            cancel.notified().await;
                            responder.respond(PromptResponse::new(StopReason::Cancelled))
                        })?;
                        Ok(())
                    },
                    on_receive_request!(),
                )
                .on_receive_notification(
                    async move |notification: CancelNotification, _connection| {
                        *cancel_observations.cancel_session.lock().unwrap() =
                            Some(notification.session_id);
                        notification_cancel.notify_waiters();
                        Ok(())
                    },
                    on_receive_notification!(),
                )
                .on_receive_request(
                    async move |request: CloseSessionRequest, responder, _connection| {
                        *close_observations.close_session.lock().unwrap() =
                            Some(request.session_id);
                        responder.respond(CloseSessionResponse::new())
                    },
                    on_receive_request!(),
                )
                .connect_to(agent_transport)
                .await;
        });

        let (protocol, _channels) = AcpProtocol::connect_transport(
            client_transport,
            AcpConnectConfig::new(Duration::from_millis(250)),
        )
        .await
        .expect("connect protocol");
        let cwd = std::env::temp_dir().join("assetiweave-acp-typed-test");
        let session = protocol.new_session(cwd.clone()).await.unwrap().session_id;
        protocol
            .set_model(session.clone(), "vendor/model", Duration::from_millis(250))
            .await
            .expect("set model");

        let prompt = protocol.prompt(session.clone(), "translate this".to_owned());
        tokio::pin!(prompt);
        tokio::select! {
            result = &mut prompt => panic!("prompt completed before cancel: {result:?}"),
            _ = async {
                loop {
                    if observations.prompt.lock().unwrap().is_some() {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            } => {}
        }
        protocol.cancel(session.clone()).expect("send cancel");
        let response = tokio::time::timeout(Duration::from_millis(250), &mut prompt)
            .await
            .expect("cancelled prompt timeout")
            .expect("cancelled prompt response");
        assert_eq!(response.stop_reason, StopReason::Cancelled);
        assert!(protocol
            .close_session(session.clone())
            .await
            .unwrap()
            .is_some());

        assert_eq!(*observations.cwd.lock().unwrap(), Some(cwd));
        assert_eq!(
            *observations.model.lock().unwrap(),
            Some((
                session.clone(),
                "model".to_owned(),
                "vendor/model".to_owned()
            ))
        );
        let (prompt_session, blocks) = observations.prompt.lock().unwrap().take().unwrap();
        assert_eq!(prompt_session, session);
        assert_eq!(blocks.len(), 1);
        let ContentBlock::Text(text) = &blocks[0] else {
            panic!("expected typed text block")
        };
        assert_eq!(text.text, "translate this");
        assert_eq!(
            *observations.cancel_session.lock().unwrap(),
            Some(session.clone())
        );
        assert_eq!(
            *observations.close_session.lock().unwrap(),
            Some(session.clone())
        );
        protocol.shutdown(Duration::from_millis(250)).await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn model_timeout_is_local_and_the_caller_can_stop_before_prompt() {
        let prompt_count = Arc::new(AtomicUsize::new(0));
        let (client_transport, agent_transport) = Channel::duplex();
        let agent_prompt_count = Arc::clone(&prompt_count);
        tokio::spawn(async move {
            let _ = Agent
                .builder()
                .on_receive_request(
                    async |_request: InitializeRequest, responder, _connection| {
                        responder.respond(InitializeResponse::new(ProtocolVersion::V1))
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async |_request: NewSessionRequest, responder, _connection| {
                        responder.respond(NewSessionResponse::new("session-timeout"))
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async |_request: SetSessionConfigOptionRequest, _responder, _connection| {
                        std::future::pending::<Result<(), agent_client_protocol::Error>>().await
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async move |_request: PromptRequest, responder, _connection| {
                        agent_prompt_count.fetch_add(1, Ordering::SeqCst);
                        responder.respond(PromptResponse::new(StopReason::EndTurn))
                    },
                    on_receive_request!(),
                )
                .connect_to(agent_transport)
                .await;
        });

        let (protocol, _channels) = AcpProtocol::connect_transport(
            client_transport,
            AcpConnectConfig::new(Duration::from_millis(250)),
        )
        .await
        .unwrap();
        let session = protocol
            .new_session(std::env::temp_dir())
            .await
            .unwrap()
            .session_id;
        let result = protocol
            .set_model(session, "slow-model", Duration::from_millis(30))
            .await;

        assert!(matches!(
            result,
            Err(AcpError::RequestTimeout {
                operation: AcpOperation::SetModel,
                ..
            })
        ));
        assert_eq!(prompt_count.load(Ordering::SeqCst), 0);
        protocol.shutdown(Duration::from_millis(250)).await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn permission_is_cancelled_and_exposed_without_raw_tool_input() {
        let (client_transport, agent_transport) = Channel::duplex();
        tokio::spawn(async move {
            let _ = Agent
                .builder()
                .on_receive_request(
                    async |_request: InitializeRequest, responder, _connection| {
                        responder.respond(InitializeResponse::new(ProtocolVersion::V1))
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async |request: NewSessionRequest, responder, _connection| {
                        responder.respond(NewSessionResponse::new(
                            request.cwd.to_string_lossy().to_string(),
                        ))
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async |request: PromptRequest, responder, connection| {
                        let request_connection = connection.clone();
                        connection.spawn(async move {
                            let tool_update = ToolCallUpdate::new(
                                "tool-secret",
                                ToolCallUpdateFields::new()
                                    .raw_input(serde_json::json!({"token": "RAW_SECRET"})),
                            );
                            let permission = RequestPermissionRequest::new(
                                request.session_id,
                                tool_update,
                                vec![PermissionOption::new(
                                    "reject",
                                    "Reject",
                                    PermissionOptionKind::RejectOnce,
                                )],
                            );
                            let response = request_connection
                                .send_request(permission)
                                .block_task()
                                .await?;
                            assert_eq!(response.outcome, RequestPermissionOutcome::Cancelled);
                            responder.respond(PromptResponse::new(StopReason::Cancelled))
                        })?;
                        Ok(())
                    },
                    on_receive_request!(),
                )
                .connect_to(agent_transport)
                .await;
        });

        let (protocol, mut channels) = AcpProtocol::connect_transport(
            client_transport,
            AcpConnectConfig::new(Duration::from_millis(250)),
        )
        .await
        .unwrap();
        let session = protocol
            .new_session(std::env::temp_dir())
            .await
            .unwrap()
            .session_id;
        let response = tokio::time::timeout(
            Duration::from_millis(250),
            protocol.prompt(session.clone(), "permission".to_owned()),
        )
        .await
        .expect("permission flow timeout")
        .expect("permission flow response");
        assert_eq!(response.stop_reason, StopReason::Cancelled);
        let event = channels.events.recv().await.expect("permission event");
        assert_eq!(
            event,
            AcpRuntimeEvent::PermissionRequested {
                session_id: session
            }
        );
        assert!(!format!("{event:?}").contains("RAW_SECRET"));
        protocol.shutdown(Duration::from_millis(250)).await.unwrap();
    }

    #[test]
    fn tool_event_mapping_drops_raw_input_and_other_content() {
        let secret = "RAW_TOOL_SECRET";
        let update = SessionUpdate::ToolCall(
            ToolCall::new("tool", "read")
                .raw_input(serde_json::json!({"secret": secret}))
                .raw_output(serde_json::json!({"secret": secret})),
        );
        let event = normalize_session_notification(SessionNotification::new("session", update));

        assert_eq!(
            event,
            AcpRuntimeEvent::ToolActivity {
                session_id: SessionId::new("session")
            }
        );
        assert!(!format!("{event:?}").contains(secret));
    }

    fn initialized_agent_transport(initialize: InitializeResponse) -> Channel {
        let (client_transport, agent_transport) = Channel::duplex();
        tokio::spawn(async move {
            let _ = Agent
                .builder()
                .on_receive_request(
                    async move |_request: InitializeRequest, responder, _connection| {
                        responder.respond(initialize.clone())
                    },
                    on_receive_request!(),
                )
                .connect_to(agent_transport)
                .await;
        });
        client_transport
    }

    #[allow(dead_code)]
    fn initialize_with_close_capability() -> InitializeResponse {
        let session = SessionCapabilities::new().close(SessionCloseCapabilities::new());
        InitializeResponse::new(ProtocolVersion::V1)
            .agent_capabilities(AgentCapabilities::new().session_capabilities(session))
    }
}
