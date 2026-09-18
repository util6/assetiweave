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
    pub(crate) async fn create_team(&self, input: CreateTeamInput) -> AppResult<TeamDetail> {
        validate_team_agent_bindings(&self.agent_runtime, &input.members)?;
        let tenant_id = self.tenant_id();
        let pool = self.db.pool();
        create_team_sqlx(pool, &tenant_id, &input).await
    }

    pub(crate) async fn get_team(&self, team_id: &str) -> AppResult<Option<TeamDetail>> {
        let tenant_id = self.tenant_id();
        let pool = self.db.pool();
        get_team_detail_sqlx(pool, &tenant_id, team_id).await
    }

    pub(crate) async fn list_teams(&self) -> AppResult<Vec<TeamDetail>> {
        let tenant_id = self.tenant_id();
        let pool = self.db.pool();
        list_teams_sqlx(pool, &tenant_id).await
    }

    pub(crate) async fn update_team(&self, input: UpdateTeamInput) -> AppResult<TeamDetail> {
        validate_team_agent_bindings(&self.agent_runtime, &input.members)?;
        let tenant_id = self.tenant_id();
        let pool = self.db.pool();
        update_team_sqlx(pool, &tenant_id, &input).await
    }

    pub(crate) async fn delete_team(&self, team_id: &str) -> AppResult<()> {
        let tenant_id = self.tenant_id();
        let pool = self.db.pool();
        delete_team_sqlx(pool, &tenant_id, team_id).await
    }
}

#[cfg(test)]
#[path = "team_tests.rs"]
mod tests;
