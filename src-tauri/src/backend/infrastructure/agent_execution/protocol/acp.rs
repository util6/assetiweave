use std::{
    fmt,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

pub(crate) use super::acp_runtime::*;

pub(crate) use super::acp_runtime::*;
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

    pub(crate) fn supports_stdio_mcp(
        &self,
        agent_id: &crate::backend::domain::agents::definition::AgentId,
    ) -> bool {
        if agent_id.as_str() == "opencode" {
            return false;
        }
        true
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

#[cfg(test)]
#[path = "acp_tests.rs"]
mod tests;
