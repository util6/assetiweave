use crate::backend::{
    application::AppService,
    models::{CreateTeamInput, TeamDetail, UpdateTeamInput},
    runtime::AppResult,
    store::{
        create_team_sqlx, delete_team_sqlx, get_team_detail_sqlx, list_teams_sqlx, update_team_sqlx,
    },
};

fn validate_team_agent_bindings(
    runtime: &std::sync::Arc<dyn crate::backend::ai_execution::AgentExecutionRuntime>,
    members: &[crate::backend::models::TeamMemberInput],
) -> AppResult<()> {
    let catalog = runtime.list_agent_catalog();
    if catalog.is_empty() {
        return Ok(());
    }
    for member in members {
        let Some(entry) = catalog
            .iter()
            .find(|entry| entry.id == member.agent_id.trim())
        else {
            return Err(crate::backend::runtime::AppError::Validation(format!(
                "Agent is not installed or enabled in Agent Market: {}",
                member.agent_id
            )));
        };
        let missing_capabilities = entry.capabilities.missing_team_capabilities();
        if !missing_capabilities.is_empty() {
            return Err(crate::backend::runtime::AppError::Domain {
                code: "team_agent_capabilities_missing".to_string(),
                message: format!(
                    "Agent {} is missing required Team Session capabilities.",
                    entry.id
                ),
                retryable: false,
                details: Some(serde_json::json!({
                    "agentId": entry.id,
                    "missingCapabilities": missing_capabilities,
                })),
            });
        }
    }
    Ok(())
}

impl AppService {
    pub(crate) fn create_team(&self, input: CreateTeamInput) -> AppResult<TeamDetail> {
        validate_team_agent_bindings(&self.agent_runtime, &input.members)?;
        let tenant_id = self.tenant_id();
        let pool = self.db.pool().clone();
        self.runtime
            .run_sync(async move { create_team_sqlx(&pool, &tenant_id, &input).await })
    }

    pub(crate) fn get_team(&self, team_id: &str) -> AppResult<Option<TeamDetail>> {
        let tenant_id = self.tenant_id();
        let pool = self.db.pool().clone();
        let team_id = team_id.to_string();
        self.runtime
            .run_sync(async move { get_team_detail_sqlx(&pool, &tenant_id, &team_id).await })
    }

    pub(crate) fn list_teams(&self) -> AppResult<Vec<TeamDetail>> {
        let tenant_id = self.tenant_id();
        let pool = self.db.pool().clone();
        self.runtime
            .run_sync(async move { list_teams_sqlx(&pool, &tenant_id).await })
    }

    pub(crate) fn update_team(&self, input: UpdateTeamInput) -> AppResult<TeamDetail> {
        validate_team_agent_bindings(&self.agent_runtime, &input.members)?;
        let tenant_id = self.tenant_id();
        let pool = self.db.pool().clone();
        self.runtime
            .run_sync(async move { update_team_sqlx(&pool, &tenant_id, &input).await })
    }

    pub(crate) fn delete_team(&self, team_id: &str) -> AppResult<()> {
        let tenant_id = self.tenant_id();
        let pool = self.db.pool().clone();
        let team_id = team_id.to_string();
        self.runtime
            .run_sync(async move { delete_team_sqlx(&pool, &tenant_id, &team_id).await })
    }
}

#[cfg(test)]
mod tests {
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
}
