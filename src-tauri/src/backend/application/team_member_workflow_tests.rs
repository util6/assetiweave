use crate::backend::{
    agents::types::{AgentCatalogEntry, AgentId, AgentProtocol, DeclaredAgentCapabilities},
    ai_execution::{
        executor::BackendFuture, AiExecutionError, AiExecutionRequest, AiExecutionResult,
        SessionEvent, SessionEventDelivery, SessionEventIdentity, SessionEventKind,
    },
    application::AppService,
    models::{CreateTeamInput, TeamMemberInput, TeamMemberTurnInput, TeamRole},
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

#[tokio::test(flavor = "multi_thread")]
async fn member_turn_start_returns_before_provider_finishes_and_exposes_events() {
    let fixture = FixtureRuntime::new();
    let service = fixture.open_service("member-turn").await;
    let team = fixture.create_team(&service, "team-member-turn").await;
    let member = &team.members[1];

    let started_at = Instant::now();
    let initial = service
        .start_member_turn(TeamMemberTurnInput {
            team_id: team.team.id.clone(),
            member_id: member.id.clone(),
            message: "SECRET_PROMPT".to_string(),
            replay: false,
        })
        .await
        .expect("start member turn");

    assert!(started_at.elapsed() < Duration::from_millis(200));
    assert!(initial.task.state.is_active());
    assert_eq!(initial.member_id, member.id);
    assert_eq!(
        initial.stream.event_count, 1,
        "the user acknowledgement is live"
    );
    assert!(!serde_json::to_string(&initial.task.detail)
        .expect("serialize task detail")
        .contains("SECRET_PROMPT"));

    fixture.wait_until_started().await;
    let streamed = service
        .get_member_stream(&team.team.id, &member.id, &initial.execution_id)
        .await
        .expect("read member stream")
        .expect("stream exists");
    assert!(streamed
        .stream
        .items
        .iter()
        .any(|item| item.text.as_deref() == Some("provider delta")));

    fixture.release();
    fixture
        .wait_until_terminal(&service, &initial.task.task_id)
        .await;
    let terminal = service
        .get_member_stream(&team.team.id, &member.id, &initial.execution_id)
        .await
        .expect("read terminal stream")
        .expect("terminal stream exists");
    assert!(terminal.task.state.is_terminal());
    assert!(!serde_json::to_string(&terminal.task.result)
        .expect("serialize task result")
        .contains("provider delta"));
}

#[tokio::test(flavor = "multi_thread")]
async fn member_turn_continues_without_a_consumer_and_cancel_is_scoped_to_one_member() {
    let fixture = FixtureRuntime::new();
    let service = fixture.open_service("member-turn-cancel").await;
    let team = fixture
        .create_team(&service, "team-member-turn-cancel")
        .await;

    let first = service
        .start_member_turn(TeamMemberTurnInput {
            team_id: team.team.id.clone(),
            member_id: team.members[1].id.clone(),
            message: "first".to_string(),
            replay: false,
        })
        .await
        .expect("start first turn");
    let second = service
        .start_member_turn(TeamMemberTurnInput {
            team_id: team.team.id.clone(),
            member_id: team.members[2].id.clone(),
            message: "second".to_string(),
            replay: false,
        })
        .await
        .expect("start second turn");

    fixture.wait_until_started_count(2).await;
    let cancelled = service
        .cancel_member_turn(&team.team.id, &team.members[1].id, &first.execution_id)
        .await
        .expect("cancel first turn");
    assert!(cancelled.task.state.is_active());

    fixture
        .wait_until_terminal(&service, &first.task.task_id)
        .await;
    let first_terminal = service
        .get_member_stream(&team.team.id, &team.members[1].id, &first.execution_id)
        .await
        .expect("read cancelled stream")
        .expect("cancelled stream exists");
    assert_eq!(
        first_terminal.task.state,
        crate::backend::runtime::tasks::TaskState::Canceled
    );
    assert!(first_terminal
        .stream
        .items
        .iter()
        .any(|item| item.kind == crate::backend::ai_execution::SessionItemKind::Cancelled));

    let second_live = service
        .get_member_stream(&team.team.id, &team.members[2].id, &second.execution_id)
        .await
        .expect("read second stream")
        .expect("second stream exists");
    assert!(second_live.task.state.is_active());
    fixture.release();
    fixture
        .wait_until_terminal(&service, &second.task.task_id)
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn member_replay_uses_the_same_workflow_and_marks_provider_events_as_replay() {
    let fixture = FixtureRuntime::new();
    let service = fixture.open_service("member-replay").await;
    let team = fixture.create_team(&service, "team-member-replay").await;
    let member = &team.members[1];
    fixture.seed_binding(&service, member).await;

    let initial = service
        .start_member_turn(TeamMemberTurnInput {
            team_id: team.team.id.clone(),
            member_id: member.id.clone(),
            message: String::new(),
            replay: true,
        })
        .await
        .expect("start member replay");

    assert!(initial.task.state.is_active());
    assert_eq!(initial.stream.event_count, 0);
    fixture.wait_until_started().await;
    let streamed = service
        .get_member_stream(&team.team.id, &member.id, &initial.execution_id)
        .await
        .expect("read replay stream")
        .expect("replay stream exists");
    assert!(streamed.stream.items.iter().any(|item| {
        item.text.as_deref() == Some("provider delta")
            && item.delivery == SessionEventDelivery::Replay
    }));

    fixture.release();
    fixture
        .wait_until_terminal(&service, &initial.task.task_id)
        .await;
    let terminal = service
        .get_member_stream(&team.team.id, &member.id, &initial.execution_id)
        .await
        .expect("read replay terminal stream")
        .expect("replay terminal stream exists");
    assert!(terminal.task.state.is_terminal());
    assert!(terminal
        .stream
        .items
        .iter()
        .all(|item| item.delivery == SessionEventDelivery::Replay));
}

#[tokio::test(flavor = "multi_thread")]
async fn member_turn_rejects_cross_team_member_missing_capability_and_missing_anchor() {
    let fixture = FixtureRuntime::new();
    let service = fixture.open_service("member-turn-validation").await;
    let team = fixture.create_team(&service, "team-validation").await;
    let other_team = fixture.create_team(&service, "other-team").await;

    let cross_team = service
        .start_member_turn(TeamMemberTurnInput {
            team_id: team.team.id.clone(),
            member_id: other_team.members[1].id.clone(),
            message: "message".to_string(),
            replay: false,
        })
        .await;
    assert_eq!(
        cross_team.expect_err("cross team member must fail").code(),
        "not_found"
    );

    fixture.set_capabilities(DeclaredAgentCapabilities {
        live_events: false,
        ..fixture.capabilities()
    });
    let missing_capability = service
        .start_member_turn(TeamMemberTurnInput {
            team_id: team.team.id.clone(),
            member_id: team.members[1].id.clone(),
            message: "message".to_string(),
            replay: false,
        })
        .await;
    assert_eq!(
        missing_capability
            .expect_err("missing live capability must fail")
            .code(),
        "team_member_capabilities_missing"
    );

    fixture.set_capabilities(fixture.capabilities());
    let missing_anchor = service
        .start_member_turn(TeamMemberTurnInput {
            team_id: team.team.id.clone(),
            member_id: team.members[1].id.clone(),
            message: String::new(),
            replay: true,
        })
        .await;
    assert_eq!(
        missing_anchor
            .expect_err("replay without a provider anchor must fail")
            .code(),
        "team_member_anchor_unavailable"
    );
}

#[test]
fn member_stream_transport_retains_tool_payload_and_keeps_sequence() {
    let public = super::public_session_snapshot(crate::backend::ai_execution::SessionSnapshot {
        revision: 7,
        event_count: 1,
        items: vec![crate::backend::ai_execution::SessionItemSnapshot {
            identity: crate::backend::ai_execution::SessionItemIdentity {
                session_id: "session".to_string(),
                member_id: "member".to_string(),
                execution_id: "execution".to_string(),
                turn_id: "turn".to_string(),
                item_id: "tool".to_string(),
            },
            kind: crate::backend::ai_execution::SessionItemKind::Tool,
            sequence: 7,
            delivery: SessionEventDelivery::Live,
            state: crate::backend::ai_execution::SessionItemState::Completed,
            partial: false,
            truncation: None,
            text: Some("RAW_TOOL_PAYLOAD".to_string()),
            status: None,
            code: None,
            tool_call_id: Some("call-1".to_string()),
            tool_name: Some("read_tool".to_string()),
            tool_input: None,
            tool_output: None,
        }],
    });

    assert_eq!(public.revision, 7);
    assert_eq!(public.items[0].text.as_deref(), Some("RAW_TOOL_PAYLOAD"));
    assert_eq!(public.items[0].tool_call_id.as_deref(), Some("call-1"));
    assert_eq!(public.items[0].tool_name.as_deref(), Some("read_tool"));
    assert!(serde_json::to_string(&public)
        .expect("serialize public stream")
        .contains("RAW_TOOL_PAYLOAD"));
}

struct FixtureRuntime {
    started: Arc<Mutex<usize>>,
    release: Arc<AtomicBool>,
    capabilities: Arc<Mutex<DeclaredAgentCapabilities>>,
}

impl FixtureRuntime {
    fn new() -> Self {
        Self {
            started: Arc::new(Mutex::new(0)),
            release: Arc::new(AtomicBool::new(false)),
            capabilities: Arc::new(Mutex::new(all_capabilities())),
        }
    }

    async fn open_service(&self, name: &str) -> AppService {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-t06-{name}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&root).expect("create fixture root");
        let runtime: Arc<dyn crate::backend::ai_execution::AgentExecutionRuntime> =
            Arc::new(self.clone_for_runtime());
        AppService::open_with_db_path_and_runtime(root.join("app.db"), runtime)
            .await
            .expect("open fixture service")
    }

    fn clone_for_runtime(&self) -> BlockingRuntime {
        BlockingRuntime {
            started: self.started.clone(),
            release: self.release.clone(),
            capabilities: self.capabilities.clone(),
        }
    }

    async fn create_team(
        &self,
        service: &AppService,
        id: &str,
    ) -> crate::backend::models::TeamDetail {
        service
            .create_team(CreateTeamInput {
                id: Some(id.to_string()),
                name: id.to_string(),
                description: None,
                members: vec![
                    TeamMemberInput {
                        id: Some(format!("{id}-leader")),
                        role: TeamRole::Leader,
                        sort_order: Some(0),
                        agent_id: "fixture-agent".to_string(),
                        model: None,
                    },
                    TeamMemberInput {
                        id: Some(format!("{id}-a")),
                        role: TeamRole::Teammate,
                        sort_order: Some(1),
                        agent_id: "fixture-agent".to_string(),
                        model: None,
                    },
                    TeamMemberInput {
                        id: Some(format!("{id}-b")),
                        role: TeamRole::Teammate,
                        sort_order: Some(2),
                        agent_id: "fixture-agent".to_string(),
                        model: None,
                    },
                ],
            })
            .await
            .expect("create Team")
    }

    async fn seed_binding(
        &self,
        service: &AppService,
        member: &crate::backend::models::TeamMember,
    ) {
        let binding = crate::backend::ai_execution::PersistentExecutionBinding {
            tenant_id: service.tenant_id().to_string(),
            execution_context_key: member.execution_context_key.clone(),
            provider_session_id: "provider-anchor".to_string(),
            agent_id: member.agent_id.clone(),
            installation_id: None,
            model: member.model.clone(),
            workspace_path: "/fixture/workspace".to_string(),
            binding_version: 1,
            provider_metadata_json: "{}".to_string(),
        };
        let store =
            crate::backend::ai_execution::PersistentBindingStore::new(service.db.pool().clone());
        store.save(&binding).await.expect("save provider binding");
    }

    async fn wait_until_started(&self) {
        self.wait_until_started_count(1).await;
    }

    async fn wait_until_started_count(&self, expected: usize) {
        for _ in 0..200 {
            if *self.started.lock().expect("started lock") >= expected {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("fixture provider did not start");
    }

    async fn wait_until_terminal(&self, service: &AppService, task_id: &str) {
        for _ in 0..200 {
            if service
                .get_member_turn_task(task_id)
                .expect("read member task")
                .is_some_and(|task| task.state.is_terminal())
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("member task did not become terminal: {task_id}");
    }

    fn release(&self) {
        self.release.store(true, Ordering::Release);
    }

    fn capabilities(&self) -> DeclaredAgentCapabilities {
        self.capabilities.lock().expect("capability lock").clone()
    }

    fn set_capabilities(&self, capabilities: DeclaredAgentCapabilities) {
        *self.capabilities.lock().expect("capability lock") = capabilities;
    }
}

impl Clone for FixtureRuntime {
    fn clone(&self) -> Self {
        Self {
            started: self.started.clone(),
            release: self.release.clone(),
            capabilities: self.capabilities.clone(),
        }
    }
}

struct BlockingRuntime {
    started: Arc<Mutex<usize>>,
    release: Arc<AtomicBool>,
    capabilities: Arc<Mutex<DeclaredAgentCapabilities>>,
}

impl crate::backend::ai_execution::AgentExecutionRuntime for BlockingRuntime {
    fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
        let started = self.started.clone();
        let release = self.release.clone();
        Box::pin(async move {
            if let Some(progress) = request.progress.as_ref() {
                progress.emit_session_event(event(
                    &request.execution_id,
                    SessionEventKind::AssistantTextDelta {
                        text: "provider delta".to_string(),
                    },
                ));
            }
            *started.lock().expect("started lock") += 1;
            while !release.load(Ordering::Acquire) && !request.cancellation.is_cancelled() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            if request.cancellation.is_cancelled() {
                return Err(AiExecutionError::Cancelled {
                    program: PathBuf::from("fixture-agent"),
                });
            }
            Ok(AiExecutionResult {
                text: "provider delta".to_string(),
                agent_id: request.agent_id,
                protocol: AgentProtocol::Acp,
                requested_model: request.model,
                elapsed_ms: 1,
                persistent_binding: None,
                replay_text: request.replay.then(|| "replayed".to_string()),
                session_cleanup: crate::backend::ai_execution::SessionCleanupStatus::Deleted,
            })
        })
    }

    fn list_agent_catalog(&self) -> Vec<AgentCatalogEntry> {
        vec![AgentCatalogEntry {
            id: "fixture-agent".to_string(),
            display_name: "Fixture Agent".to_string(),
            command: "fixture-agent".to_string(),
            args: Vec::new(),
            availability_command: "fixture-agent".to_string(),
            protocol: "acp".to_string(),
            capabilities: self.capabilities.lock().expect("capability lock").clone(),
        }]
    }

    fn agent_capabilities(&self, _agent_id: &AgentId) -> Option<DeclaredAgentCapabilities> {
        Some(self.capabilities.lock().expect("capability lock").clone())
    }
}

fn event(execution_id: &str, kind: SessionEventKind) -> SessionEvent {
    SessionEvent {
        identity: SessionEventIdentity {
            session_id: "provider-session".to_string(),
            member_id: "provider-member".to_string(),
            execution_id: "provider-execution".to_string(),
            turn_id: "provider-turn".to_string(),
            item_id: "assistant".to_string(),
            event_id: format!("provider-event-{execution_id}"),
        },
        sequence: 1,
        delivery: SessionEventDelivery::Live,
        kind,
        truncation: None,
    }
}

fn all_capabilities() -> DeclaredAgentCapabilities {
    DeclaredAgentCapabilities {
        text_prompt: true,
        resume: true,
        history_replay: true,
        live_events: true,
        rich_history_replay: false,
        team_tools: false,
        resume_args: None,
    }
}
