use chrono::Utc;
use sqlx::{sqlite::SqliteRow, Row as SqlxRow, SqlitePool};
use uuid::Uuid;

use crate::backend::{
    models::{CreateTeamInput, Team, TeamDetail, TeamMember, TeamRole, UpdateTeamInput},
    runtime::{AppError, AppResult},
};

use super::codec::{decode_enum_app, encode_enum_app};

pub(crate) fn validate_team_roster_members(
    members: &[crate::backend::models::TeamMemberInput],
) -> AppResult<()> {
    if members.is_empty() {
        return Err(AppError::Validation("Team must have members".to_string()));
    }

    let mut leader_count = 0;
    let mut teammate_count = 0;

    let mut member_ids = std::collections::HashSet::new();
    for (index, member) in members.iter().enumerate() {
        if member.agent_id.trim().is_empty() {
            return Err(AppError::Validation(format!(
                "Team member at index {} requires a valid agent_id",
                index
            )));
        }
        if let Some(id) = member
            .id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            if !member_ids.insert(id.to_string()) {
                return Err(AppError::Validation(format!(
                    "Team member id is duplicated: {id}"
                )));
            }
        }
        match member.role {
            TeamRole::Leader => leader_count += 1,
            TeamRole::Teammate => teammate_count += 1,
        }
    }

    if leader_count != 1 {
        return Err(AppError::Validation(format!(
            "Team must have exactly one leader, found {}",
            leader_count
        )));
    }

    if teammate_count == 0 {
        return Err(AppError::Validation(
            "Team must have at least one teammate".to_string(),
        ));
    }

    Ok(())
}

fn normalized_member_model(input: &crate::backend::models::TeamMemberInput) -> Option<String> {
    input
        .model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToString::to_string)
}

fn map_team_row(row: &SqliteRow) -> AppResult<Team> {
    Ok(Team {
        id: row.try_get("id").map_err(AppError::external)?,
        name: row.try_get("name").map_err(AppError::external)?,
        description: row.try_get("description").map_err(AppError::external)?,
        created_at: row.try_get("created_at").map_err(AppError::external)?,
        updated_at: row.try_get("updated_at").map_err(AppError::external)?,
    })
}

fn map_team_member_row(row: &SqliteRow) -> AppResult<TeamMember> {
    let role_raw: String = row.try_get("role").map_err(AppError::external)?;
    let role: TeamRole = decode_enum_app(role_raw)?;

    Ok(TeamMember {
        id: row.try_get("id").map_err(AppError::external)?,
        team_id: row.try_get("team_id").map_err(AppError::external)?,
        role,
        sort_order: row.try_get("sort_order").map_err(AppError::external)?,
        agent_id: row.try_get("agent_id").map_err(AppError::external)?,
        model: row.try_get("model").map_err(AppError::external)?,
        execution_context_key: row
            .try_get("execution_context_key")
            .map_err(AppError::external)?,
        created_at: row.try_get("created_at").map_err(AppError::external)?,
        updated_at: row.try_get("updated_at").map_err(AppError::external)?,
    })
}

pub(crate) async fn create_team_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    input: &CreateTeamInput,
) -> AppResult<TeamDetail> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::Validation(
            "Team name must not be empty".to_string(),
        ));
    }

    validate_team_roster_members(&input.members)?;

    let team_id = input
        .id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("team-{}", Uuid::new_v4().simple()));

    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(AppError::external)?;

    sqlx::query(
        r#"
        INSERT INTO teams (tenant_id, id, name, description, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?5)
        "#,
    )
    .bind(tenant_id)
    .bind(&team_id)
    .bind(name)
    .bind(input.description.as_deref().map(str::trim))
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;

    let mut members = Vec::with_capacity(input.members.len());

    for (index, member_input) in input.members.iter().enumerate() {
        let member_id = member_input
            .id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("member-{}", Uuid::new_v4().simple()));

        // The array is the write contract.  Client supplied sort_order is
        // accepted for backwards compatibility but never becomes authority.
        let sort_order = index as i32;
        let execution_context_key = format!("ctx-{}", Uuid::new_v4().simple());
        let role_str = encode_enum_app(member_input.role)?;

        sqlx::query(
            r#"
            INSERT INTO team_members (
                tenant_id, team_id, id, role, sort_order,
                agent_id, model, execution_context_key, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
            "#,
        )
        .bind(tenant_id)
        .bind(&team_id)
        .bind(&member_id)
        .bind(&role_str)
        .bind(sort_order)
        .bind(member_input.agent_id.trim())
        .bind(normalized_member_model(member_input))
        .bind(&execution_context_key)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;

        members.push(TeamMember {
            id: member_id,
            team_id: team_id.clone(),
            role: member_input.role,
            sort_order,
            agent_id: member_input.agent_id.trim().to_string(),
            model: normalized_member_model(member_input),
            execution_context_key,
            created_at: now.clone(),
            updated_at: now.clone(),
        });
    }

    tx.commit().await.map_err(AppError::external)?;

    Ok(TeamDetail {
        team: Team {
            id: team_id,
            name: name.to_string(),
            description: input
                .description
                .as_deref()
                .map(str::trim)
                .map(ToString::to_string),
            created_at: now.clone(),
            updated_at: now,
        },
        members,
    })
}

pub(crate) async fn get_team_detail_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    team_id: &str,
) -> AppResult<Option<TeamDetail>> {
    let team_row = sqlx::query(
        r#"
        SELECT id, name, description, created_at, updated_at
        FROM teams
        WHERE tenant_id = ?1 AND id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(team_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;

    let Some(team_row) = team_row else {
        return Ok(None);
    };

    let team = map_team_row(&team_row)?;

    let member_rows = sqlx::query(
        r#"
        SELECT id, team_id, role, sort_order, agent_id, model, execution_context_key, created_at, updated_at
        FROM team_members
        WHERE tenant_id = ?1 AND team_id = ?2
        ORDER BY sort_order ASC, created_at ASC
        "#,
    )
    .bind(tenant_id)
    .bind(team_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    let mut members = Vec::with_capacity(member_rows.len());
    for row in &member_rows {
        members.push(map_team_member_row(row)?);
    }

    Ok(Some(TeamDetail { team, members }))
}

pub(crate) async fn list_teams_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<TeamDetail>> {
    let team_rows = sqlx::query(
        r#"
        SELECT id, name, description, created_at, updated_at
        FROM teams
        WHERE tenant_id = ?1
        ORDER BY created_at DESC
        "#,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    let mut results = Vec::with_capacity(team_rows.len());
    for team_row in &team_rows {
        let team = map_team_row(team_row)?;
        let member_rows = sqlx::query(
            r#"
            SELECT id, team_id, role, sort_order, agent_id, model, execution_context_key, created_at, updated_at
            FROM team_members
            WHERE tenant_id = ?1 AND team_id = ?2
            ORDER BY sort_order ASC, created_at ASC
            "#,
        )
        .bind(tenant_id)
        .bind(&team.id)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;

        let mut members = Vec::with_capacity(member_rows.len());
        for row in &member_rows {
            members.push(map_team_member_row(row)?);
        }

        results.push(TeamDetail { team, members });
    }

    Ok(results)
}

pub(crate) async fn update_team_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    input: &UpdateTeamInput,
) -> AppResult<TeamDetail> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::Validation(
            "Team name must not be empty".to_string(),
        ));
    }

    validate_team_roster_members(&input.members)?;

    let mut tx = pool.begin().await.map_err(AppError::external)?;

    let active_run: Option<String> = sqlx::query_scalar(
        "SELECT id FROM team_runs WHERE tenant_id = ?1 AND team_id = ?2 AND state IN ('drafting', 'awaiting_review', 'executing') LIMIT 1",
    )
    .bind(tenant_id)
    .bind(&input.team_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::external)?;
    if let Some(run_id) = active_run {
        return Err(AppError::Conflict(format!(
            "Team roster is frozen while run {run_id} is active"
        )));
    }

    // Verify team exists
    let existing_team = sqlx::query(
        r#"
        SELECT id, created_at
        FROM teams
        WHERE tenant_id = ?1 AND id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(&input.team_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::external)?;

    let Some(existing_team_row) = existing_team else {
        return Err(AppError::Validation(format!(
            "Team not found: {}",
            input.team_id
        )));
    };

    let team_created_at: String = existing_team_row
        .try_get("created_at")
        .map_err(AppError::external)?;
    let now = Utc::now().to_rfc3339();

    // Update team header
    sqlx::query(
        r#"
        UPDATE teams
        SET name = ?1, description = ?2, updated_at = ?3
        WHERE tenant_id = ?4 AND id = ?5
        "#,
    )
    .bind(name)
    .bind(input.description.as_deref().map(str::trim))
    .bind(&now)
    .bind(tenant_id)
    .bind(&input.team_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;

    // Load existing members to preserve stable execution_context_key and created_at
    let existing_member_rows = sqlx::query(
        r#"
        SELECT id, execution_context_key, created_at
        FROM team_members
        WHERE tenant_id = ?1 AND team_id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(&input.team_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(AppError::external)?;

    let mut existing_members_map = std::collections::HashMap::new();
    for row in &existing_member_rows {
        let id: String = row.try_get("id").map_err(AppError::external)?;
        let context_key: String = row
            .try_get("execution_context_key")
            .map_err(AppError::external)?;
        let created_at: String = row.try_get("created_at").map_err(AppError::external)?;
        existing_members_map.insert(id, (context_key, created_at));
    }

    // Delete current members in this transaction to rewrite with updated roster
    sqlx::query(
        r#"
        DELETE FROM team_members
        WHERE tenant_id = ?1 AND team_id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(&input.team_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;

    let mut members = Vec::with_capacity(input.members.len());

    for (index, member_input) in input.members.iter().enumerate() {
        let member_id = member_input
            .id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("member-{}", Uuid::new_v4().simple()));

        let (execution_context_key, created_at) =
            if let Some((existing_key, existing_created)) = existing_members_map.get(&member_id) {
                (existing_key.clone(), existing_created.clone())
            } else {
                (format!("ctx-{}", Uuid::new_v4().simple()), now.clone())
            };

        let sort_order = index as i32;
        let role_str = encode_enum_app(member_input.role)?;

        sqlx::query(
            r#"
            INSERT INTO team_members (
                tenant_id, team_id, id, role, sort_order,
                agent_id, model, execution_context_key, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
        )
        .bind(tenant_id)
        .bind(&input.team_id)
        .bind(&member_id)
        .bind(&role_str)
        .bind(sort_order)
        .bind(member_input.agent_id.trim())
        .bind(normalized_member_model(member_input))
        .bind(&execution_context_key)
        .bind(&created_at)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;

        members.push(TeamMember {
            id: member_id,
            team_id: input.team_id.clone(),
            role: member_input.role,
            sort_order,
            agent_id: member_input.agent_id.trim().to_string(),
            model: normalized_member_model(member_input),
            execution_context_key,
            created_at,
            updated_at: now.clone(),
        });
    }

    tx.commit().await.map_err(AppError::external)?;

    Ok(TeamDetail {
        team: Team {
            id: input.team_id.clone(),
            name: name.to_string(),
            description: input
                .description
                .as_deref()
                .map(str::trim)
                .map(ToString::to_string),
            created_at: team_created_at,
            updated_at: now,
        },
        members,
    })
}

pub(crate) async fn delete_team_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    team_id: &str,
) -> AppResult<()> {
    let mut tx = pool.begin().await.map_err(AppError::external)?;

    let active_run: Option<String> = sqlx::query_scalar(
        "SELECT id FROM team_runs WHERE tenant_id = ?1 AND team_id = ?2 AND state IN ('drafting', 'awaiting_review', 'executing') LIMIT 1",
    )
    .bind(tenant_id)
    .bind(team_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::external)?;
    if let Some(run_id) = active_run {
        return Err(AppError::Conflict(format!(
            "Team cannot be deleted while run {run_id} is active"
        )));
    }

    sqlx::query(
        r#"
        DELETE FROM team_members
        WHERE tenant_id = ?1 AND team_id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(team_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;

    sqlx::query(
        r#"
        DELETE FROM teams
        WHERE tenant_id = ?1 AND id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(team_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;

    tx.commit().await.map_err(AppError::external)?;
    Ok(())
}

fn parse_run_state(value: String) -> AppResult<crate::backend::models::TeamRunState> {
    match value.as_str() {
        "drafting" => Ok(crate::backend::models::TeamRunState::Drafting),
        "awaiting_review" => Ok(crate::backend::models::TeamRunState::AwaitingReview),
        "executing" => Ok(crate::backend::models::TeamRunState::Executing),
        "terminal" => Ok(crate::backend::models::TeamRunState::Terminal),
        _ => Err(AppError::external("invalid TeamRun state")),
    }
}

fn parse_task_state(value: String) -> AppResult<crate::backend::models::TeamTaskState> {
    match value.as_str() {
        "draft" => Ok(crate::backend::models::TeamTaskState::Draft),
        "queued" => Ok(crate::backend::models::TeamTaskState::Queued),
        "running" => Ok(crate::backend::models::TeamTaskState::Running),
        "succeeded" => Ok(crate::backend::models::TeamTaskState::Succeeded),
        "failed" => Ok(crate::backend::models::TeamTaskState::Failed),
        "canceled" => Ok(crate::backend::models::TeamTaskState::Canceled),
        _ => Err(AppError::external("invalid TeamTask state")),
    }
}

fn map_run_row(row: &SqliteRow) -> AppResult<crate::backend::models::TeamRun> {
    let roster_json: String = row
        .try_get("roster_snapshot_json")
        .map_err(AppError::external)?;
    Ok(crate::backend::models::TeamRun {
        id: row.try_get("id").map_err(AppError::external)?,
        team_id: row.try_get("team_id").map_err(AppError::external)?,
        state: parse_run_state(row.try_get("state").map_err(AppError::external)?)?,
        revision: row.try_get("revision").map_err(AppError::external)?,
        leader_member_id: row
            .try_get("leader_member_id")
            .map_err(AppError::external)?,
        roster_snapshot: serde_json::from_str(&roster_json).map_err(AppError::external)?,
        created_at: row.try_get("created_at").map_err(AppError::external)?,
        updated_at: row.try_get("updated_at").map_err(AppError::external)?,
        finished_at: row.try_get("finished_at").map_err(AppError::external)?,
        error_code: row.try_get("error_code").map_err(AppError::external)?,
    })
}

fn map_task_row(row: &SqliteRow) -> AppResult<crate::backend::models::TeamTask> {
    Ok(crate::backend::models::TeamTask {
        id: row.try_get("id").map_err(AppError::external)?,
        run_id: row.try_get("run_id").map_err(AppError::external)?,
        team_id: row.try_get("team_id").map_err(AppError::external)?,
        title: row.try_get("title").map_err(AppError::external)?,
        description: row.try_get("description").map_err(AppError::external)?,
        sort_order: row.try_get("sort_order").map_err(AppError::external)?,
        recommended_member_id: row
            .try_get("recommended_member_id")
            .map_err(AppError::external)?,
        owner_member_id: row.try_get("owner_member_id").map_err(AppError::external)?,
        state: parse_task_state(row.try_get("state").map_err(AppError::external)?)?,
        revision: row.try_get("revision").map_err(AppError::external)?,
        result: row.try_get("result").map_err(AppError::external)?,
        error_code: row.try_get("error_code").map_err(AppError::external)?,
        created_at: row.try_get("created_at").map_err(AppError::external)?,
        updated_at: row.try_get("updated_at").map_err(AppError::external)?,
    })
}

async fn load_team_tasks_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
) -> AppResult<Vec<crate::backend::models::TeamTask>> {
    let rows = sqlx::query(
        "SELECT id, run_id, team_id, title, description, sort_order, recommended_member_id, owner_member_id, state, revision, result, error_code, created_at, updated_at FROM team_tasks WHERE tenant_id = ?1 AND run_id = ?2 ORDER BY sort_order ASC, created_at ASC",
    )
    .bind(tenant_id)
    .bind(run_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    rows.iter().map(map_task_row).collect()
}

pub(crate) async fn get_team_run_snapshot_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
) -> AppResult<Option<crate::backend::models::TeamRunSnapshot>> {
    let row = sqlx::query(
        "SELECT id, team_id, state, revision, leader_member_id, roster_snapshot_json, created_at, updated_at, finished_at, error_code FROM team_runs WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(run_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let run = map_run_row(&row)?;
    let tasks = load_team_tasks_sqlx(pool, tenant_id, run_id).await?;
    let unread_mailbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM team_mailbox_messages WHERE tenant_id = ?1 AND run_id = ?2 AND recipient_member_id = ?3 AND acked_at IS NULL",
    )
    .bind(tenant_id)
    .bind(run_id)
    .bind(&run.leader_member_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::external)?;
    Ok(Some(crate::backend::models::TeamRunSnapshot {
        run,
        tasks,
        unread_mailbox_count: unread_mailbox_count as usize,
    }))
}

pub(crate) async fn get_latest_team_run_snapshot_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    team_id: &str,
) -> AppResult<Option<crate::backend::models::TeamRunSnapshot>> {
    let run_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM team_runs WHERE tenant_id = ?1 AND team_id = ?2 ORDER BY CASE WHEN state IN ('drafting', 'awaiting_review', 'executing') THEN 0 ELSE 1 END, updated_at DESC, id DESC LIMIT 1",
    )
    .bind(tenant_id)
    .bind(team_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;
    match run_id {
        Some(run_id) => get_team_run_snapshot_sqlx(pool, tenant_id, &run_id).await,
        None => Ok(None),
    }
}

/// Creates the durable shell before contacting a provider.  This makes draft
/// generation an observable background operation instead of a synchronous
/// application call that can hold a request open for the whole provider turn.
pub(crate) async fn create_team_run_shell_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    team_id: &str,
) -> AppResult<crate::backend::models::TeamRunSnapshot> {
    let team = get_team_detail_sqlx(pool, tenant_id, team_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Team not found: {team_id}")))?;
    let leader = team
        .members
        .iter()
        .find(|member| member.role == TeamRole::Leader)
        .ok_or_else(|| AppError::Validation("Team has no leader".to_string()))?;
    let roster_snapshot = team
        .members
        .iter()
        .map(|member| crate::backend::models::TeamRosterSnapshotMember {
            member_id: member.id.clone(),
            role: member.role,
            sort_order: member.sort_order,
            agent_id: member.agent_id.clone(),
            model: member.model.clone(),
            execution_context_key: member.execution_context_key.clone(),
        })
        .collect::<Vec<_>>();
    let roster_json = serde_json::to_string(&roster_snapshot).map_err(AppError::external)?;
    let run_id = format!("run-{}", Uuid::new_v4().simple());
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let active: Option<String> = sqlx::query_scalar(
        "SELECT id FROM team_runs WHERE tenant_id = ?1 AND team_id = ?2 AND state IN ('drafting', 'awaiting_review', 'executing') LIMIT 1",
    )
    .bind(tenant_id)
    .bind(team_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::external)?;
    if let Some(active) = active {
        return Err(AppError::Conflict(format!(
            "Team already has an active run: {active}"
        )));
    }
    sqlx::query("INSERT INTO team_runs (tenant_id, id, team_id, state, revision, leader_member_id, roster_snapshot_json, created_at, updated_at) VALUES (?1, ?2, ?3, 'drafting', 1, ?4, ?5, ?6, ?6)")
        .bind(tenant_id)
        .bind(&run_id)
        .bind(team_id)
        .bind(&leader.id)
        .bind(&roster_json)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)?;
    get_team_run_snapshot_sqlx(pool, tenant_id, &run_id)
        .await?
        .ok_or_else(|| AppError::external("Team run disappeared after shell creation"))
}

pub(crate) async fn complete_team_run_draft_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
    drafts: &[crate::backend::models::TeamTaskDraft],
) -> AppResult<crate::backend::models::TeamRunSnapshot> {
    if drafts.is_empty() {
        return Err(AppError::Validation(
            "Team draft must contain at least one task".to_string(),
        ));
    }
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let row = sqlx::query("SELECT team_id, state, roster_snapshot_json FROM team_runs WHERE tenant_id = ?1 AND id = ?2")
        .bind(tenant_id)
        .bind(run_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::external)?
        .ok_or_else(|| AppError::NotFound(format!("Team run not found: {run_id}")))?;
    let team_id: String = row.try_get("team_id").map_err(AppError::external)?;
    let state: String = row.try_get("state").map_err(AppError::external)?;
    if state != "drafting" {
        return Err(AppError::Conflict(
            "Team run is no longer drafting".to_string(),
        ));
    }
    let roster: Vec<crate::backend::models::TeamRosterSnapshotMember> = serde_json::from_str(
        &row.try_get::<String, _>("roster_snapshot_json")
            .map_err(AppError::external)?,
    )
    .map_err(AppError::external)?;
    let teammate_ids = roster
        .iter()
        .filter(|member| member.role == TeamRole::Teammate)
        .map(|member| member.member_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut task_ids = std::collections::HashSet::new();
    let normalized_ids = drafts
        .iter()
        .map(|draft| {
            draft
                .id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("task-{}", Uuid::new_v4().simple()))
        })
        .collect::<Vec<_>>();
    for (draft, task_id) in drafts.iter().zip(&normalized_ids) {
        if draft.title.trim().is_empty() || draft.description.trim().is_empty() {
            return Err(AppError::Validation(
                "Team task title and description are required".to_string(),
            ));
        }
        let owner = draft.recommended_member_id.trim();
        if !teammate_ids.contains(owner) {
            return Err(AppError::Validation(format!(
                "Recommended member is not a teammate: {owner}"
            )));
        }
        if !task_ids.insert(task_id.clone()) {
            return Err(AppError::Validation(
                "Team draft contains duplicate task identities".to_string(),
            ));
        }
    }
    let now = Utc::now().to_rfc3339();
    for (index, (draft, task_id)) in drafts.iter().zip(&normalized_ids).enumerate() {
        sqlx::query("INSERT INTO team_tasks (tenant_id, id, run_id, team_id, title, description, sort_order, recommended_member_id, owner_member_id, state, revision, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, 'draft', 1, ?9, ?9)")
            .bind(tenant_id)
            .bind(&task_id)
            .bind(run_id)
            .bind(&team_id)
            .bind(draft.title.trim())
            .bind(draft.description.trim())
            .bind(index as i32)
            .bind(draft.recommended_member_id.trim())
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;
    }
    sqlx::query("UPDATE team_runs SET state = 'awaiting_review', revision = revision + 1, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3")
        .bind(&now)
        .bind(tenant_id)
        .bind(run_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)?;
    get_team_run_snapshot_sqlx(pool, tenant_id, run_id)
        .await?
        .ok_or_else(|| AppError::external("Team run disappeared after draft completion"))
}

pub(crate) async fn fail_team_run_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
    error_code: &str,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE team_runs SET state = 'terminal', error_code = ?1, finished_at = ?2, revision = revision + 1, updated_at = ?2 WHERE tenant_id = ?3 AND id = ?4 AND state = 'drafting'")
        .bind(error_code)
        .bind(&now)
        .bind(tenant_id)
        .bind(run_id)
        .execute(pool)
        .await
        .map_err(AppError::external)?;
    Ok(())
}

/// Converge every unfinished task and its terminal mailbox notification in one
/// transaction.  A cancelled worker must leave durable facts that a restart
/// can observe instead of leaving a run permanently executing.
pub(crate) async fn cancel_team_run_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
    error_code: &str,
) -> AppResult<()> {
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let task_rows = sqlx::query(
        "SELECT id, team_id, run_id, owner_member_id FROM team_tasks WHERE tenant_id = ?1 AND run_id = ?2 AND state IN ('queued', 'running')",
    )
    .bind(tenant_id)
    .bind(run_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(AppError::external)?;
    let now = Utc::now().to_rfc3339();
    for row in task_rows {
        let task_id: String = row.try_get("id").map_err(AppError::external)?;
        let team_id: String = row.try_get("team_id").map_err(AppError::external)?;
        let owner: String = row
            .try_get::<Option<String>, _>("owner_member_id")
            .map_err(AppError::external)?
            .ok_or_else(|| AppError::Validation("Team task has no owner".to_string()))?;
        sqlx::query(
            "UPDATE team_tasks SET state = 'canceled', error_code = ?1, revision = revision + 1, updated_at = ?2 WHERE tenant_id = ?3 AND id = ?4 AND state IN ('queued', 'running')",
        )
        .bind(error_code)
        .bind(&now)
        .bind(tenant_id)
        .bind(&task_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
        let idempotency = format!("task-terminal:{task_id}");
        let body = serde_json::json!({
            "task_id": task_id,
            "state": "canceled",
            "error_code": error_code,
        })
        .to_string();
        sqlx::query(
            "INSERT OR IGNORE INTO team_mailbox_messages (tenant_id, id, team_id, run_id, task_id, sender_member_id, recipient_member_id, message_type, body, idempotency_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, (SELECT leader_member_id FROM team_runs WHERE tenant_id = ?1 AND id = ?4), 'task_terminal', ?7, ?8, ?9)",
        )
        .bind(tenant_id)
        .bind(format!("mail-{}", Uuid::new_v4().simple()))
        .bind(team_id)
        .bind(run_id)
        .bind(task_id)
        .bind(owner)
        .bind(body)
        .bind(idempotency)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
    }
    sqlx::query(
        "UPDATE team_runs SET state = 'terminal', error_code = ?1, finished_at = COALESCE(finished_at, ?2), revision = revision + 1, updated_at = ?2 WHERE tenant_id = ?3 AND id = ?4 AND state IN ('drafting', 'awaiting_review', 'executing')",
    )
    .bind(error_code)
    .bind(&now)
    .bind(tenant_id)
    .bind(run_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)
}

/// Returns runs that still need coordinator work after a process restart.
/// Terminal runs with unacknowledged Leader mailbox facts are included because
/// a crash may happen after the last task commit but before summary consume.
pub(crate) async fn list_recoverable_team_run_ids_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<String>> {
    sqlx::query_scalar(
        "SELECT DISTINCT runs.id FROM team_runs AS runs LEFT JOIN team_mailbox_messages AS mailbox ON mailbox.tenant_id = runs.tenant_id AND mailbox.run_id = runs.id AND mailbox.recipient_member_id = runs.leader_member_id AND mailbox.acked_at IS NULL WHERE runs.tenant_id = ?1 AND (runs.state = 'executing' OR (runs.state = 'terminal' AND mailbox.id IS NOT NULL)) ORDER BY runs.updated_at ASC",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)
}

pub(crate) async fn review_team_run_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    input: &crate::backend::models::TeamReviewInput,
) -> AppResult<crate::backend::models::TeamRunSnapshot> {
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let run_state: Option<(String, i64, String, String)> = sqlx::query_as("SELECT state, revision, leader_member_id, roster_snapshot_json FROM team_runs WHERE tenant_id = ?1 AND id = ?2")
        .bind(tenant_id).bind(&input.run_id).fetch_optional(&mut *tx).await.map_err(AppError::external)?;
    let Some((state, revision, leader_id, roster_json)) = run_state else {
        return Err(AppError::NotFound(format!(
            "Team run not found: {}",
            input.run_id
        )));
    };
    if state != "awaiting_review" {
        return Err(AppError::Conflict(
            "Team run is no longer awaiting review".to_string(),
        ));
    }
    if revision != input.revision {
        return Err(AppError::Conflict("Team run revision is stale".to_string()));
    }
    let roster: Vec<crate::backend::models::TeamRosterSnapshotMember> =
        serde_json::from_str(&roster_json).map_err(AppError::external)?;
    let task_rows =
        sqlx::query("SELECT id FROM team_tasks WHERE tenant_id = ?1 AND run_id = ?2 ORDER BY id")
            .bind(tenant_id)
            .bind(&input.run_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(AppError::external)?;
    let expected = task_rows
        .iter()
        .map(|row| row.try_get::<String, _>("id"))
        .collect::<Result<std::collections::HashSet<_>, _>>()
        .map_err(AppError::external)?;
    let mut supplied = std::collections::HashSet::new();
    for task in &input.tasks {
        if !expected.contains(&task.task_id)
            || !supplied.insert(task.task_id.clone())
            || task.owner_member_id == leader_id
        {
            return Err(AppError::Validation(
                "Review must assign every task to a distinct teammate owner".to_string(),
            ));
        }
        if !roster.iter().any(|member| {
            member.member_id == task.owner_member_id
                && member.role == crate::backend::models::TeamRole::Teammate
        }) {
            return Err(AppError::Validation(
                "Task owner must be a teammate in the frozen roster".to_string(),
            ));
        }
    }
    if supplied != expected {
        return Err(AppError::Validation(
            "Review must include every Team task".to_string(),
        ));
    }
    for (index, task) in input.tasks.iter().enumerate() {
        sqlx::query("UPDATE team_tasks SET owner_member_id = ?1, sort_order = ?2, revision = revision + 1, updated_at = ?3 WHERE tenant_id = ?4 AND id = ?5")
            .bind(&task.owner_member_id).bind(index as i32).bind(Utc::now().to_rfc3339()).bind(tenant_id).bind(&task.task_id).execute(&mut *tx).await.map_err(AppError::external)?;
    }
    sqlx::query("UPDATE team_runs SET revision = revision + 1, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3")
        .bind(Utc::now().to_rfc3339()).bind(tenant_id).bind(&input.run_id).execute(&mut *tx).await.map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)?;
    get_team_run_snapshot_sqlx(pool, tenant_id, &input.run_id)
        .await?
        .ok_or_else(|| AppError::external("Team run disappeared after review"))
}

pub(crate) async fn confirm_team_run_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    input: &crate::backend::models::TeamConfirmInput,
) -> AppResult<crate::backend::models::TeamRunSnapshot> {
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let row = sqlx::query(
        "SELECT team_id, state, revision FROM team_runs WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(&input.run_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::external)?;
    let Some(row) = row else {
        return Err(AppError::NotFound(format!(
            "Team run not found: {}",
            input.run_id
        )));
    };
    let team_id: String = row.try_get("team_id").map_err(AppError::external)?;
    let state: String = row.try_get("state").map_err(AppError::external)?;
    let revision: i64 = row.try_get("revision").map_err(AppError::external)?;
    if state != "awaiting_review" {
        return Err(AppError::Conflict(
            "Only a run awaiting review can be confirmed".to_string(),
        ));
    }
    if revision != input.revision {
        return Err(AppError::Conflict("Team run revision is stale".to_string()));
    }
    let missing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM team_tasks WHERE tenant_id = ?1 AND run_id = ?2 AND (owner_member_id IS NULL OR state <> 'draft')")
        .bind(tenant_id).bind(&input.run_id).fetch_one(&mut *tx).await.map_err(AppError::external)?;
    if missing != 0 {
        return Err(AppError::Validation(
            "Every reviewed task must remain a draft with an owner".to_string(),
        ));
    }
    sqlx::query("UPDATE team_runs SET state = 'executing', revision = revision + 1, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3")
        .bind(Utc::now().to_rfc3339()).bind(tenant_id).bind(&input.run_id).execute(&mut *tx).await.map_err(AppError::external)?;
    sqlx::query("UPDATE team_tasks SET state = 'queued', dispatch_key = ?1 || ':' || id, revision = revision + 1, updated_at = ?2 WHERE tenant_id = ?3 AND run_id = ?1")
        .bind(&input.run_id).bind(Utc::now().to_rfc3339()).bind(tenant_id).execute(&mut *tx).await.map_err(AppError::external)?;
    let event =
        crate::backend::events::DomainEvent::team_run_confirmed(tenant_id, &input.run_id, &team_id);
    crate::backend::events::append_outbox_event_sqlx_tx(&mut tx, &event).await?;
    tx.commit().await.map_err(AppError::external)?;
    get_team_run_snapshot_sqlx(pool, tenant_id, &input.run_id)
        .await?
        .ok_or_else(|| AppError::external("Team run disappeared after confirmation"))
}

pub(crate) async fn claim_team_task_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    task_id: &str,
) -> AppResult<Option<crate::backend::models::TeamTask>> {
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let row = sqlx::query("SELECT id, run_id, team_id, title, description, sort_order, recommended_member_id, owner_member_id, state, revision, result, error_code, created_at, updated_at FROM team_tasks WHERE tenant_id = ?1 AND id = ?2")
        .bind(tenant_id).bind(task_id).fetch_optional(&mut *tx).await.map_err(AppError::external)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let task = map_task_row(&row)?;
    if !matches!(
        task.state,
        crate::backend::models::TeamTaskState::Queued
            | crate::backend::models::TeamTaskState::Running
    ) {
        return Ok(None);
    }
    let dispatch_key = format!("{}:{task_id}", task.run_id);
    let inserted = sqlx::query("INSERT OR IGNORE INTO team_task_claims (tenant_id, task_id, dispatch_key, claimed_at) VALUES (?1, ?2, ?3, ?4)")
        .bind(tenant_id).bind(task_id).bind(&dispatch_key).bind(Utc::now().to_rfc3339()).execute(&mut *tx).await.map_err(AppError::external)?;
    if inserted.rows_affected() > 0 && task.state == crate::backend::models::TeamTaskState::Queued {
        sqlx::query("UPDATE team_tasks SET state = 'running', revision = revision + 1, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3 AND state = 'queued'")
            .bind(Utc::now().to_rfc3339()).bind(tenant_id).bind(task_id).execute(&mut *tx).await.map_err(AppError::external)?;
    }
    if inserted.rows_affected() == 0 && task.state == crate::backend::models::TeamTaskState::Queued
    {
        return Ok(None);
    }
    tx.commit().await.map_err(AppError::external)?;
    let task = get_team_task_sqlx(pool, tenant_id, task_id).await?;
    Ok(task)
}

pub(crate) async fn get_team_task_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    task_id: &str,
) -> AppResult<Option<crate::backend::models::TeamTask>> {
    let row = sqlx::query("SELECT id, run_id, team_id, title, description, sort_order, recommended_member_id, owner_member_id, state, revision, result, error_code, created_at, updated_at FROM team_tasks WHERE tenant_id = ?1 AND id = ?2")
        .bind(tenant_id).bind(task_id).fetch_optional(pool).await.map_err(AppError::external)?;
    row.as_ref().map(map_task_row).transpose()
}

pub(crate) async fn mark_team_task_running_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    task_id: &str,
) -> AppResult<crate::backend::models::TeamTask> {
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let row = sqlx::query("SELECT run_id, state FROM team_tasks WHERE tenant_id = ?1 AND id = ?2")
        .bind(tenant_id)
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::external)?
        .ok_or_else(|| AppError::NotFound(format!("Team task not found: {task_id}")))?;
    let run_id: String = row.try_get("run_id").map_err(AppError::external)?;
    let state: String = row.try_get("state").map_err(AppError::external)?;
    let now = Utc::now().to_rfc3339();
    let dispatch_key = format!("{run_id}:{task_id}");
    if state == "running" {
        let claim_exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM team_task_claims WHERE tenant_id = ?1 AND task_id = ?2",
        )
        .bind(tenant_id)
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::external)?;
        if claim_exists.is_none() {
            sqlx::query(
                "INSERT OR IGNORE INTO team_task_claims (tenant_id, task_id, dispatch_key, claimed_at) VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(tenant_id)
            .bind(task_id)
            .bind(&dispatch_key)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;
        }
        tx.commit().await.map_err(AppError::external)?;
        return get_team_task_sqlx(pool, tenant_id, task_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Team task not found: {task_id}")));
    }
    if state != "queued" {
        return Err(AppError::Conflict(
            "Team task is not queued or has already been claimed".to_string(),
        ));
    }
    let claim = sqlx::query(
        "INSERT OR IGNORE INTO team_task_claims (tenant_id, task_id, dispatch_key, claimed_at) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(tenant_id)
    .bind(task_id)
    .bind(&dispatch_key)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    if claim.rows_affected() == 0 {
        return Err(AppError::Conflict(
            "Team task is not queued or has already been claimed".to_string(),
        ));
    }
    sqlx::query(
        "UPDATE team_tasks SET state = 'running', revision = revision + 1, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3 AND state = 'queued'",
    )
    .bind(&now)
    .bind(tenant_id)
    .bind(task_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)?;
    get_team_task_sqlx(pool, tenant_id, task_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Team task not found: {task_id}")))
}

pub(crate) async fn finish_team_task_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    task_id: &str,
    state: crate::backend::models::TeamTaskState,
    result: Option<&str>,
    error_code: Option<&str>,
) -> AppResult<crate::backend::models::TeamTask> {
    if !state.is_terminal() {
        return Err(AppError::Validation(
            "Team task finish requires a terminal state".to_string(),
        ));
    }
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let task = sqlx::query(
        "SELECT run_id, team_id, owner_member_id FROM team_tasks WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(task_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::external)?;
    let Some(task) = task else {
        return Err(AppError::NotFound(format!(
            "Team task not found: {task_id}"
        )));
    };
    let run_id: String = task.try_get("run_id").map_err(AppError::external)?;
    let team_id: String = task.try_get("team_id").map_err(AppError::external)?;
    let owner: String = task
        .try_get::<Option<String>, _>("owner_member_id")
        .map_err(AppError::external)?
        .ok_or_else(|| AppError::Validation("Team task has no owner".to_string()))?;
    let now = Utc::now().to_rfc3339();
    let updated = sqlx::query("UPDATE team_tasks SET state = ?1, result = ?2, error_code = ?3, revision = revision + 1, updated_at = ?4 WHERE tenant_id = ?5 AND id = ?6 AND state IN ('running', 'queued')")
        .bind(state.as_str()).bind(result).bind(error_code).bind(&now).bind(tenant_id).bind(task_id).execute(&mut *tx).await.map_err(AppError::external)?;
    if updated.rows_affected() == 0 {
        let current = sqlx::query("SELECT id, run_id, team_id, title, description, sort_order, recommended_member_id, owner_member_id, state, revision, result, error_code, created_at, updated_at FROM team_tasks WHERE tenant_id = ?1 AND id = ?2")
            .bind(tenant_id)
            .bind(task_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::external)?
            .map(|row| map_task_row(&row))
            .transpose()?;
        if let Some(current) = current.filter(|task| task.state.is_terminal()) {
            tx.rollback().await.map_err(AppError::external)?;
            return Ok(current);
        }
        return Err(AppError::Conflict(
            "Team task is not running or has already finished".to_string(),
        ));
    }
    let message_id = format!("mail-{}", Uuid::new_v4().simple());
    let idempotency = format!("task-terminal:{task_id}");
    let body = serde_json::json!({ "task_id": task_id, "state": state.as_str(), "result": result, "error_code": error_code }).to_string();
    sqlx::query("INSERT OR IGNORE INTO team_mailbox_messages (tenant_id, id, team_id, run_id, task_id, sender_member_id, recipient_member_id, message_type, body, idempotency_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, (SELECT leader_member_id FROM team_runs WHERE tenant_id = ?1 AND id = ?4), 'task_terminal', ?7, ?8, ?9)")
        .bind(tenant_id).bind(&message_id).bind(&team_id).bind(&run_id).bind(task_id).bind(&owner).bind(&body).bind(&idempotency).bind(&now).execute(&mut *tx).await.map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)?;
    get_team_task_sqlx(pool, tenant_id, task_id)
        .await?
        .ok_or_else(|| AppError::external("Team task disappeared after finish"))
}

/// Marks an executing run terminal after its mailbox summary attempt. Keeping
/// the run executing until that point makes terminal snapshots truthful: a
/// consumer never observes a completed run with an unprocessed terminal task
/// mailbox merely because the worker has not reached its summary step yet.
pub(crate) async fn mark_team_run_terminal_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    run_id: &str,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE team_runs SET state = 'terminal', finished_at = COALESCE(finished_at, ?1), revision = revision + 1, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3 AND state = 'executing' AND NOT EXISTS (SELECT 1 FROM team_tasks WHERE tenant_id = ?2 AND run_id = ?3 AND state NOT IN ('succeeded', 'failed', 'canceled'))",
    )
    .bind(&now)
    .bind(tenant_id)
    .bind(run_id)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    Ok(())
}

pub(crate) async fn send_team_mailbox_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    input: &crate::backend::models::TeamMailboxSendInput,
) -> AppResult<crate::backend::models::TeamMailboxMessage> {
    if input.body.trim().is_empty() || input.idempotency_key.trim().is_empty() {
        return Err(AppError::Validation(
            "Mailbox body and idempotency key are required".to_string(),
        ));
    }
    let id = format!("mail-{}", Uuid::new_v4().simple());
    let now = Utc::now().to_rfc3339();
    sqlx::query("INSERT OR IGNORE INTO team_mailbox_messages (tenant_id, id, team_id, run_id, task_id, sender_member_id, recipient_member_id, message_type, body, idempotency_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)")
        .bind(tenant_id).bind(&id).bind(&input.team_id).bind(&input.run_id).bind(&input.task_id).bind(&input.sender_member_id).bind(&input.recipient_member_id).bind(&input.message_type).bind(input.body.trim()).bind(input.idempotency_key.trim()).bind(&now).execute(pool).await.map_err(AppError::external)?;
    let row = sqlx::query("SELECT id, team_id, run_id, task_id, sender_member_id, recipient_member_id, message_type, body, created_at, read_at, acked_at FROM team_mailbox_messages WHERE tenant_id = ?1 AND id = (SELECT id FROM team_mailbox_messages WHERE tenant_id = ?1 AND idempotency_key = ?2)")
        .bind(tenant_id).bind(input.idempotency_key.trim()).fetch_one(pool).await.map_err(AppError::external)?;
    Ok(crate::backend::models::TeamMailboxMessage {
        id: row.try_get("id").map_err(AppError::external)?,
        team_id: row.try_get("team_id").map_err(AppError::external)?,
        run_id: row.try_get("run_id").map_err(AppError::external)?,
        task_id: row.try_get("task_id").map_err(AppError::external)?,
        sender_member_id: row
            .try_get("sender_member_id")
            .map_err(AppError::external)?,
        recipient_member_id: row
            .try_get("recipient_member_id")
            .map_err(AppError::external)?,
        message_type: row.try_get("message_type").map_err(AppError::external)?,
        body: row.try_get("body").map_err(AppError::external)?,
        created_at: row.try_get("created_at").map_err(AppError::external)?,
        read_at: row.try_get("read_at").map_err(AppError::external)?,
        acked_at: row.try_get("acked_at").map_err(AppError::external)?,
    })
}

pub(crate) async fn read_team_mailbox_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    input: &crate::backend::models::TeamMailboxReadInput,
) -> AppResult<Vec<crate::backend::models::TeamMailboxMessage>> {
    let now = Utc::now().to_rfc3339();
    if input.ack {
        sqlx::query("UPDATE team_mailbox_messages SET read_at = COALESCE(read_at, ?1), acked_at = ?1 WHERE tenant_id = ?2 AND team_id = ?3 AND run_id = ?4 AND recipient_member_id = ?5 AND acked_at IS NULL")
            .bind(&now).bind(tenant_id).bind(&input.team_id).bind(&input.run_id).bind(&input.recipient_member_id).execute(pool).await.map_err(AppError::external)?;
    } else {
        sqlx::query("UPDATE team_mailbox_messages SET read_at = COALESCE(read_at, ?1) WHERE tenant_id = ?2 AND team_id = ?3 AND run_id = ?4 AND recipient_member_id = ?5 AND read_at IS NULL")
            .bind(&now).bind(tenant_id).bind(&input.team_id).bind(&input.run_id).bind(&input.recipient_member_id).execute(pool).await.map_err(AppError::external)?;
    }
    let query = if input.ack {
        "SELECT id, team_id, run_id, task_id, sender_member_id, recipient_member_id, message_type, body, created_at, read_at, acked_at FROM team_mailbox_messages WHERE tenant_id = ?1 AND team_id = ?2 AND run_id = ?3 AND recipient_member_id = ?4 ORDER BY created_at ASC"
    } else {
        "SELECT id, team_id, run_id, task_id, sender_member_id, recipient_member_id, message_type, body, created_at, read_at, acked_at FROM team_mailbox_messages WHERE tenant_id = ?1 AND team_id = ?2 AND run_id = ?3 AND recipient_member_id = ?4 AND acked_at IS NULL ORDER BY created_at ASC"
    };
    let rows = sqlx::query(query)
        .bind(tenant_id)
        .bind(&input.team_id)
        .bind(&input.run_id)
        .bind(&input.recipient_member_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    rows.iter()
        .map(|row| {
            Ok(crate::backend::models::TeamMailboxMessage {
                id: row.try_get("id").map_err(AppError::external)?,
                team_id: row.try_get("team_id").map_err(AppError::external)?,
                run_id: row.try_get("run_id").map_err(AppError::external)?,
                task_id: row.try_get("task_id").map_err(AppError::external)?,
                sender_member_id: row
                    .try_get("sender_member_id")
                    .map_err(AppError::external)?,
                recipient_member_id: row
                    .try_get("recipient_member_id")
                    .map_err(AppError::external)?,
                message_type: row.try_get("message_type").map_err(AppError::external)?,
                body: row.try_get("body").map_err(AppError::external)?,
                created_at: row.try_get("created_at").map_err(AppError::external)?,
                read_at: row.try_get("read_at").map_err(AppError::external)?,
                acked_at: row.try_get("acked_at").map_err(AppError::external)?,
            })
        })
        .collect()
}

pub(crate) async fn create_team_tool_credential_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    credential_hash: &str,
    input: &crate::backend::models::TeamToolCredentialInput,
    expires_at: &str,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO team_tool_credentials (tenant_id, credential_hash, team_id, run_id, member_id, expires_at, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")
        .bind(tenant_id).bind(credential_hash).bind(&input.team_id).bind(&input.run_id).bind(&input.member_id).bind(expires_at).bind(&now)
        .execute(pool).await.map_err(AppError::external)?;
    Ok(())
}

pub(crate) async fn authenticate_team_tool_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    credential_hash: &str,
    team_id: &str,
    run_id: &str,
    member_id: &str,
) -> AppResult<bool> {
    let now = Utc::now().to_rfc3339();
    let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM team_tool_credentials WHERE tenant_id = ?1 AND credential_hash = ?2 AND team_id = ?3 AND run_id = ?4 AND member_id = ?5 AND expires_at > ?6")
        .bind(tenant_id).bind(credential_hash).bind(team_id).bind(run_id).bind(member_id).bind(now)
        .fetch_optional(pool).await.map_err(AppError::external)?;
    Ok(found.is_some())
}
