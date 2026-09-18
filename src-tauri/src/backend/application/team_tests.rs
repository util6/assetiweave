use super::*;
use crate::backend::{
    agents::types::{AgentCatalogEntry, AgentId, DeclaredAgentCapabilities},
    ai_execution::{
        executor::BackendFuture, AgentExecutionRuntime, AiExecutionError, AiExecutionRequest,
    },
    models::{TeamMemberInput, TeamRole},
};
use std::{collections::HashMap, sync::Arc};

struct CapabilityRuntime {
    capabilities: HashMap<String, DeclaredAgentCapabilities>,
}

impl AgentExecutionRuntime for CapabilityRuntime {
    fn execute<'a>(&'a self, _request: AiExecutionRequest) -> BackendFuture<'a> {
        Box::pin(async {
            Err(AiExecutionError::Protocol {
                operation: "team_capability_fixture",
            })
        })
    }

    fn list_agent_catalog(&self) -> Vec<AgentCatalogEntry> {
        self.capabilities
            .keys()
            .map(|id| AgentCatalogEntry {
                id: id.clone(),
                display_name: id.clone(),
                command: "fixture".to_string(),
                args: Vec::new(),
                availability_command: "fixture".to_string(),
                protocol: "native".to_string(),
                capabilities: self.capabilities.get(id).cloned().unwrap_or_default(),
            })
            .collect()
    }

    fn agent_capabilities(&self, agent_id: &AgentId) -> Option<DeclaredAgentCapabilities> {
        self.capabilities.get(agent_id.as_str()).cloned()
    }
}

#[test]
fn team_admission_uses_semantic_capabilities_for_native_members() {
    let runtime: Arc<dyn AgentExecutionRuntime> = Arc::new(CapabilityRuntime {
        capabilities: HashMap::from([
            (
                "leader".to_string(),
                DeclaredAgentCapabilities {
                    text_prompt: true,
                    resume: true,
                    history_replay: true,
                    live_events: true,
                    rich_history_replay: false,
                    team_tools: false,
                    resume_args: None,
                },
            ),
            (
                "teammate".to_string(),
                DeclaredAgentCapabilities {
                    text_prompt: true,
                    resume: true,
                    history_replay: false,
                    live_events: true,
                    rich_history_replay: false,
                    team_tools: false,
                    resume_args: None,
                },
            ),
        ]),
    });
    let members = vec![
        TeamMemberInput {
            id: Some("leader-member".to_string()),
            role: TeamRole::Leader,
            sort_order: Some(0),
            agent_id: "leader".to_string(),
            model: None,
        },
        TeamMemberInput {
            id: Some("teammate-member".to_string()),
            role: TeamRole::Teammate,
            sort_order: Some(1),
            agent_id: "teammate".to_string(),
            model: None,
        },
    ];

    validate_team_agent_bindings(&runtime, &members[..1])
        .expect("Native members with all semantic capabilities are eligible");
    let error = validate_team_agent_bindings(&runtime, &members)
        .expect_err("every Team member must satisfy semantic Session capabilities");
    match error {
        crate::backend::runtime::AppError::Domain { code, details, .. } => {
            assert_eq!(code, "team_agent_capabilities_missing");
            assert_eq!(
                details.expect("missing capabilities details")["missingCapabilities"],
                serde_json::json!(["history_replay"])
            );
        }
        other => panic!("expected structured capability error, got {other:?}"),
    }
}
