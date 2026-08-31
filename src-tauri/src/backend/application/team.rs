use crate::backend::{
    application::AppService,
    models::{CreateTeamInput, TeamDetail, UpdateTeamInput},
    runtime::AppResult,
    store::{
        create_team_sqlx, delete_team_sqlx, get_team_detail_sqlx, list_teams_sqlx, update_team_sqlx,
    },
};

impl AppService {
    pub(crate) fn create_team(&self, input: CreateTeamInput) -> AppResult<TeamDetail> {
        let tenant_id = self.tenant_id();
        let pool = self.db.pool().clone();
        self.db
            .block_on(async move { create_team_sqlx(&pool, &tenant_id, &input).await })
    }

    pub(crate) fn get_team(&self, team_id: &str) -> AppResult<Option<TeamDetail>> {
        let tenant_id = self.tenant_id();
        let pool = self.db.pool().clone();
        let team_id = team_id.to_string();
        self.db
            .block_on(async move { get_team_detail_sqlx(&pool, &tenant_id, &team_id).await })
    }

    pub(crate) fn list_teams(&self) -> AppResult<Vec<TeamDetail>> {
        let tenant_id = self.tenant_id();
        let pool = self.db.pool().clone();
        self.db
            .block_on(async move { list_teams_sqlx(&pool, &tenant_id).await })
    }

    pub(crate) fn update_team(&self, input: UpdateTeamInput) -> AppResult<TeamDetail> {
        let tenant_id = self.tenant_id();
        let pool = self.db.pool().clone();
        self.db
            .block_on(async move { update_team_sqlx(&pool, &tenant_id, &input).await })
    }

    pub(crate) fn delete_team(&self, team_id: &str) -> AppResult<()> {
        let tenant_id = self.tenant_id();
        let pool = self.db.pool().clone();
        let team_id = team_id.to_string();
        self.db
            .block_on(async move { delete_team_sqlx(&pool, &tenant_id, &team_id).await })
    }
}
