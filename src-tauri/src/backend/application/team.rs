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
        if !catalog
            .iter()
            .any(|entry| entry.id == member.agent_id.trim())
        {
            return Err(crate::backend::runtime::AppError::Validation(format!(
                "Agent is not installed or enabled in Agent Market: {}",
                member.agent_id
            )));
        }
        if let Ok(agent_id) = crate::backend::agents::types::AgentId::parse(member.agent_id.clone())
        {
            if let Some(capabilities) = runtime.agent_capabilities(&agent_id) {
                if !capabilities.resume {
                    return Err(crate::backend::runtime::AppError::Validation(format!(
                        "Team member Agent does not support persistent resume: {}",
                        member.agent_id
                    )));
                }
                if member.role == crate::backend::models::TeamRole::Leader
                    && !capabilities.history_replay
                {
                    return Err(crate::backend::runtime::AppError::Validation(format!(
                        "Team leader Agent does not support history replay: {}",
                        member.agent_id
                    )));
                }
            }
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
