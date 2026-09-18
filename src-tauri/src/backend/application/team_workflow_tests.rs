use super::*;
use crate::backend::{
    agents::types::{AgentCatalogEntry, AgentId, AgentProtocol, DeclaredAgentCapabilities},
    ai_execution::{executor::BackendFuture, AgentExecutionRuntime, AiExecutionResult},
    models::{CreateTeamInput, TeamMemberInput, TeamReviewTaskInput, TeamRole, TeamTaskState},
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
struct RequestObservation {
    purpose: AiExecutionPurpose,
    agent_id: String,
    replay: bool,
    prompt: String,
}

struct FakeTeamRuntime {
    observations: Arc<Mutex<Vec<RequestObservation>>>,
}

impl FakeTeamRuntime {
    fn new() -> (Arc<Self>, Arc<Mutex<Vec<RequestObservation>>>) {
        let observations = Arc::new(Mutex::new(Vec::new()));
        (
            Arc::new(Self {
                observations: observations.clone(),
            }),
            observations,
        )
    }
}

impl AgentExecutionRuntime for FakeTeamRuntime {
    fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
        let observations = self.observations.clone();
        Box::pin(async move {
            observations.lock().unwrap().push(RequestObservation {
                purpose: request.purpose,
                agent_id: request.agent_id.to_string(),
                replay: request.replay,
                prompt: request.prompt.clone(),
            });
            let text = match request.purpose {
                    AiExecutionPurpose::TeamDraft => {
                        r#"{"tasks":[{"id":"task-a","title":"A work","description":"Work for A","recommended_member_id":"member-a"},{"id":"task-b","title":"B work","description":"Work for B","recommended_member_id":"member-b"}]}"#.to_string()
                    }
                    AiExecutionPurpose::TeamLeaderChat if request.replay => String::new(),
                    AiExecutionPurpose::TeamLeaderChat => "leader reply".to_string(),
                    AiExecutionPurpose::TeamTask => format!("done by {}", request.agent_id),
                    AiExecutionPurpose::TeamSummary => "summary reply".to_string(),
                    _ => "unused".to_string(),
                };
            Ok(AiExecutionResult {
                text,
                agent_id: request.agent_id,
                protocol: AgentProtocol::Acp,
                requested_model: request.model,
                elapsed_ms: 1,
                persistent_binding: None,
                replay_text: request
                    .replay
                    .then(|| "replayed leader history".to_string()),
                session_cleanup: crate::backend::ai_execution::SessionCleanupStatus::Deleted,
            })
        })
    }

    fn list_agent_catalog(&self) -> Vec<AgentCatalogEntry> {
        ["leader-agent", "agent-a", "agent-b"]
            .into_iter()
            .map(|id| AgentCatalogEntry {
                id: id.to_string(),
                display_name: id.to_string(),
                command: "fixture".to_string(),
                args: Vec::new(),
                availability_command: "fixture".to_string(),
                protocol: "acp".to_string(),
                capabilities: DeclaredAgentCapabilities {
                    text_prompt: true,
                    resume: true,
                    history_replay: true,
                    live_events: true,
                    rich_history_replay: false,
                    team_tools: true,
                    resume_args: None,
                },
            })
            .collect()
    }

    fn agent_capabilities(&self, _agent_id: &AgentId) -> Option<DeclaredAgentCapabilities> {
        Some(DeclaredAgentCapabilities {
            text_prompt: true,
            resume: true,
            history_replay: true,
            live_events: true,
            rich_history_replay: false,
            team_tools: true,
            resume_args: None,
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn team_workflow_freezes_roster_requires_review_and_uses_confirmed_owners() {
    let root = std::env::temp_dir().join(format!("assetiweave-team-flow-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let (runtime, observations) = FakeTeamRuntime::new();
    let service = AppService::open_with_db_path_and_runtime(root.join("app.db"), runtime)
        .await
        .expect("open fixture service");
    let team = service
        .create_team(CreateTeamInput {
            id: Some("team-flow".to_string()),
            name: "Flow".to_string(),
            description: None,
            members: vec![
                TeamMemberInput {
                    id: Some("leader".to_string()),
                    role: TeamRole::Leader,
                    sort_order: Some(99),
                    agent_id: "leader-agent".to_string(),
                    model: None,
                },
                TeamMemberInput {
                    id: Some("member-a".to_string()),
                    role: TeamRole::Teammate,
                    sort_order: Some(1),
                    agent_id: "agent-a".to_string(),
                    model: None,
                },
                TeamMemberInput {
                    id: Some("member-b".to_string()),
                    role: TeamRole::Teammate,
                    sort_order: Some(0),
                    agent_id: "agent-b".to_string(),
                    model: None,
                },
            ],
        })
        .await
        .expect("create Team");
    assert_eq!(
        team.members
            .iter()
            .map(|member| member.sort_order)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );

    let live = service
        .leader_chat(TeamLeaderChatInput {
            team_id: team.team.id.clone(),
            message: "hello".to_string(),
            replay: false,
        })
        .await
        .expect("leader chat");
    assert_eq!(live.text, "leader reply");
    let replay = service
        .leader_chat(TeamLeaderChatInput {
            team_id: team.team.id.clone(),
            message: String::new(),
            replay: true,
        })
        .await
        .expect("leader replay");
    assert_eq!(replay.text, "replayed leader history");

    let shell = service
        .draft_team(TeamDraftInput {
            team_id: team.team.id.clone(),
            leader_message: "split the work".to_string(),
        })
        .await
        .expect("draft shell");
    assert_eq!(
        shell.run.state,
        crate::backend::models::TeamRunState::Drafting
    );
    let draft_task_id = format!("team-task-draft-{}", shell.run.id);
    wait_for_task_terminal(&service, &draft_task_id).await;
    let draft = wait_for_run_state(
        &service,
        &shell.run.id,
        crate::backend::models::TeamRunState::AwaitingReview,
    )
    .await;
    assert_eq!(draft.tasks.len(), 2);
    assert!(draft
        .tasks
        .iter()
        .all(|task| task.owner_member_id.is_none()));

    let reviewed = service
        .review_team_run(TeamReviewInput {
            run_id: draft.run.id.clone(),
            revision: draft.run.revision,
            tasks: vec![
                TeamReviewTaskInput {
                    task_id: "task-b".to_string(),
                    title: None,
                    description: None,
                    owner_member_id: "member-a".to_string(),
                    sort_order: 100,
                },
                TeamReviewTaskInput {
                    task_id: "task-a".to_string(),
                    title: None,
                    description: None,
                    owner_member_id: "member-b".to_string(),
                    sort_order: -100,
                },
            ],
        })
        .await
        .expect("review draft");
    assert_eq!(
        reviewed.tasks[0].owner_member_id.as_deref(),
        Some("member-a")
    );
    assert_eq!(
        reviewed.tasks[1].owner_member_id.as_deref(),
        Some("member-b")
    );

    let confirmed = service
        .confirm_team_run(TeamConfirmInput {
            run_id: reviewed.run.id.clone(),
            revision: reviewed.run.revision,
        })
        .await
        .expect("confirm run");
    assert_eq!(
        confirmed.run.state,
        crate::backend::models::TeamRunState::Executing
    );
    let terminal = wait_for_run_state(
        &service,
        &confirmed.run.id,
        crate::backend::models::TeamRunState::Terminal,
    )
    .await;
    assert_eq!(terminal.tasks[0].state, TeamTaskState::Succeeded);
    assert_eq!(terminal.tasks[1].state, TeamTaskState::Succeeded);
    assert_eq!(
        terminal.tasks[0].owner_member_id.as_deref(),
        Some("member-a")
    );
    assert_eq!(
        terminal.tasks[1].owner_member_id.as_deref(),
        Some("member-b")
    );
    assert_eq!(terminal.unread_mailbox_count, 0);

    let observations = observations.lock().unwrap().clone();
    let task_agents = observations
        .iter()
        .filter(|observation| observation.purpose == AiExecutionPurpose::TeamTask)
        .map(|observation| observation.agent_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(task_agents, vec!["agent-a", "agent-b"]);
    assert!(observations
        .iter()
        .filter(|observation| observation.replay)
        .all(|observation| observation.purpose == AiExecutionPurpose::TeamLeaderChat));
    assert!(observations
        .iter()
        .all(|observation| !observation.prompt.contains("team-tool-")));

    drop(service);
    std::fs::remove_dir_all(root).ok();
}

async fn wait_for_task_terminal(service: &AppService, task_id: &str) {
    for _ in 0..200 {
        if service
            .team_run_task(task_id)
            .expect("read task")
            .is_some_and(|task| task.state.is_terminal())
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("Team task did not become terminal: {task_id}");
}

async fn wait_for_run_state(
    service: &AppService,
    run_id: &str,
    state: crate::backend::models::TeamRunState,
) -> TeamRunSnapshot {
    for _ in 0..200 {
        if let Some(snapshot) = service.get_team_run(run_id).await.expect("read Team run") {
            if snapshot.run.state == state {
                return snapshot;
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("Team run did not become {state:?}: {run_id}");
}
