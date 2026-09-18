use super::*;
use crate::backend::models::{CreateTeamInput, TeamMemberInput, TeamRole};
use sqlx::sqlite::SqlitePoolOptions;

#[tokio::test]
async fn create_team_validation_fails_before_touching_database() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    sqlx::query(
        r#"
            CREATE TABLE teams (
                tenant_id TEXT NOT NULL,
                id TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (tenant_id, id)
            );
            CREATE TABLE team_members (
                tenant_id TEXT NOT NULL,
                id TEXT NOT NULL,
                team_id TEXT NOT NULL,
                role TEXT NOT NULL,
                sort_order INTEGER NOT NULL,
                agent_id TEXT NOT NULL,
                model TEXT,
                execution_context_key TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (tenant_id, id)
            );
            "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let blank_name_input = CreateTeamInput {
        id: None,
        name: "   ".to_string(),
        description: None,
        members: vec![
            TeamMemberInput {
                id: None,
                role: TeamRole::Leader,
                sort_order: Some(0),
                agent_id: "agent-1".to_string(),
                model: None,
            },
            TeamMemberInput {
                id: None,
                role: TeamRole::Teammate,
                sort_order: Some(1),
                agent_id: "agent-2".to_string(),
                model: None,
            },
        ],
    };

    let err = create_team_sqlx(&pool, "tenant-default", &blank_name_input)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "validation_error");
    assert_eq!(err.view().message, "Team name must not be empty");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM teams")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);

    let blank_member_input = CreateTeamInput {
        id: None,
        name: "Valid Name".to_string(),
        description: None,
        members: vec![
            TeamMemberInput {
                id: None,
                role: TeamRole::Leader,
                sort_order: Some(0),
                agent_id: "   ".to_string(),
                model: None,
            },
            TeamMemberInput {
                id: None,
                role: TeamRole::Teammate,
                sort_order: Some(1),
                agent_id: "agent-2".to_string(),
                model: None,
            },
        ],
    };

    let err = create_team_sqlx(&pool, "tenant-default", &blank_member_input)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "validation_error");
    assert_eq!(
        err.view().message,
        "Team member at index 0 requires a valid agent_id"
    );

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM teams")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn team_repo_uses_validator_and_deletes_manual_whitespace_checks() {
    let source = include_str!("team_repo.rs");
    assert!(!source.contains(concat!("if member.", "agent_id.trim().is_empty()")));
    assert!(source.contains("input.validate().map_err(map_team_validation_error)"));
}

#[tokio::test]
async fn typed_mailbox_row_preserves_nulls() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let row = sqlx::query_as::<_, TeamMailboxRow>(
            "SELECT 'm' AS id, 't' AS team_id, 'r' AS run_id, NULL AS task_id, 's' AS sender_member_id, 'u' AS recipient_member_id, 'note' AS message_type, 'body' AS body, '2026-09-03T00:00:00Z' AS created_at, NULL AS read_at, NULL AS acked_at",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
    let message: crate::backend::models::TeamMailboxMessage = row.into();
    assert_eq!(message.id, "m");
    assert_eq!(message.body, "body");
    assert_eq!(message.task_id, None);
    assert_eq!(message.acked_at, None);
}
