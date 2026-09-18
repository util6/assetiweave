use super::*;
use agent_client_protocol::schema::v1::{
    AgentCapabilities, CloseSessionRequest, DeleteSessionRequest, DeleteSessionResponse,
    NewSessionRequest, PermissionOption, PermissionOptionKind, PromptRequest, SessionCapabilities,
    SessionCloseCapabilities, SessionDeleteCapabilities, SetSessionConfigOptionRequest, ToolCall,
    ToolCallUpdate, ToolCallUpdateFields,
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

#[tokio::test(flavor = "current_thread")]
async fn delete_session_uses_typed_request_only_when_capability_is_advertised() {
    let transport = initialized_agent_transport(InitializeResponse::new(ProtocolVersion::V1));
    let (protocol, _channels) = AcpProtocol::connect_transport(
        transport,
        AcpConnectConfig::new(Duration::from_millis(250)),
    )
    .await
    .expect("connect protocol without delete capability");

    assert!(protocol
        .delete_session(SessionId::new("unsupported-session"))
        .await
        .expect("capability skip")
        .is_none());
    protocol.shutdown(Duration::from_millis(250)).await.unwrap();

    let deleted_session = Arc::new(Mutex::new(None));
    let observed_session = Arc::clone(&deleted_session);
    let (client_transport, agent_transport) = Channel::duplex();
    tokio::spawn(async move {
        let session = SessionCapabilities::new().delete(SessionDeleteCapabilities::new());
        let initialize = InitializeResponse::new(ProtocolVersion::V1)
            .agent_capabilities(AgentCapabilities::new().session_capabilities(session));
        let _ = Agent
            .builder()
            .on_receive_request(
                async move |_request: InitializeRequest, responder, _connection| {
                    responder.respond(initialize.clone())
                },
                on_receive_request!(),
            )
            .on_receive_request(
                async move |request: DeleteSessionRequest, responder, _connection| {
                    *observed_session.lock().unwrap() = Some(request.session_id);
                    responder.respond(DeleteSessionResponse::new())
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
    .expect("connect protocol with delete capability");

    let session_id = SessionId::new("advertised-session");
    assert!(protocol
        .delete_session(session_id.clone())
        .await
        .expect("typed session/delete")
        .is_some());
    assert_eq!(*deleted_session.lock().unwrap(), Some(session_id));
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
                    *close_observations.close_session.lock().unwrap() = Some(request.session_id);
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
fn tool_event_mapping_retains_raw_input_and_output_with_redaction() {
    let secret = "RAW_TOOL_SECRET";
    let update = SessionUpdate::ToolCall(
        ToolCall::new("tool", "read")
            .raw_input(serde_json::json!({"secret": secret}))
            .raw_output(serde_json::json!({"secret": secret})),
    );
    let event = normalize_session_notification(SessionNotification::new("session", update));

    assert_eq!(
        event,
        AcpRuntimeEvent::ToolCall {
            session_id: SessionId::new("session"),
            tool_call_id: "tool".to_string(),
            title: "read".to_string(),
            status: AcpToolStatus::Pending,
            raw_input: Some(serde_json::json!({"secret": secret})),
            raw_output: Some(serde_json::json!({"secret": secret})),
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
