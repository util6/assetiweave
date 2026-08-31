use chrono::Utc;
use sqlx::{sqlite::SqliteRow, Row as SqlxRow, SqlitePool};
use uuid::Uuid;

use crate::backend::{
    models::{CreateTeamInput, Team, TeamDetail, TeamMember, TeamRole, UpdateTeamInput},
    runtime::{AppError, AppResult},
};

use super::codec::{decode_enum_app, encode_enum_app};

fn validate_team_roster_members(
    members: &[crate::backend::models::TeamMemberInput],
) -> AppResult<()> {
    if members.is_empty() {
        return Err(AppError::Validation("Team must have members".to_string()));
    }

    let mut leader_count = 0;
    let mut teammate_count = 0;

    for (index, member) in members.iter().enumerate() {
        if member.agent_id.trim().is_empty() {
            return Err(AppError::Validation(format!(
                "Team member at index {} requires a valid agent_id",
                index
            )));
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

        let sort_order = member_input.sort_order.unwrap_or(index as i32);
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
        .bind(member_input.model.as_deref().map(str::trim))
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
            model: member_input
                .model
                .as_deref()
                .map(str::trim)
                .map(ToString::to_string),
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

        let sort_order = member_input.sort_order.unwrap_or(index as i32);
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
        .bind(member_input.model.as_deref().map(str::trim))
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
            model: member_input
                .model
                .as_deref()
                .map(str::trim)
                .map(ToString::to_string),
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
