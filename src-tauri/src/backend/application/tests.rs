use super::prelude::*;
use crate::backend::models::{
    AssetFormat, AssetGroupRules, ConversationAdapterKind, ConversationAdapterTrustState,
    ConversationGroupingOrigin, ConversationPartKind, ConversationPartRole, ConversationSource,
    ConversationSourceKind, DeploymentState, NormalizedConversationPart,
    NormalizedConversationSession, NormalizedConversationTurn, SourceKind,
};
use sqlx::AssertSqlSafe;
use std::fs;

fn execute_test_sql(service: &AppService, sql: &str) -> AppResult<()> {
    let pool = service.db.pool().clone();
    service.db.block_on(async move {
        for statement in sql.split(';').map(str::trim).filter(|sql| !sql.is_empty()) {
            sqlx::query(AssertSqlSafe(statement.to_string()))
                .execute(&pool)
                .await
                .map_err(AppError::external)?;
        }
        Ok(())
    })
}

fn clear_test_tables(service: &AppService, tables: &[&str]) {
    let pool = service.db.pool().clone();
    service
        .db
        .block_on(async move {
            for table in tables {
                let statement = format!("DELETE FROM {table}");
                sqlx::query(AssertSqlSafe(statement))
                    .execute(&pool)
                    .await
                    .map_err(AppError::external)?;
            }
            AppResult::Ok(())
        })
        .expect("clear test tables");
}

#[cfg(unix)]
#[test]
fn command_projection_falls_back_to_core_projector_for_legacy_adapter() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-command-projector-fallback-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create command projector test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let now = Utc::now().to_rfc3339();
    let legacy_adapter = ConversationAdapter {
        id: "legacy-command-adapter".to_string(),
        name: "Legacy Command Adapter".to_string(),
        kind: ConversationAdapterKind::External,
        version: "1.0.0".to_string(),
        enabled: true,
        manifest_path: None,
        executable_path: None,
        content_hash: None,
        trusted_hash: None,
        trust_state: ConversationAdapterTrustState::Trusted,
        protocol_version: Some(1),
        capabilities: vec!["read_session".to_string()],
        input_kinds: vec![ConversationSourceKind::File],
        card_contract_version: None,
        card_kinds: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    };
    let pool = service.db.pool().clone();
    service
        .db
        .block_on(async move {
            crate::backend::store::upsert_conversation_adapter_sqlx(
                &pool,
                "default",
                &legacy_adapter,
            )
            .await
        })
        .expect("save legacy conversation adapter");
    service
        .runtime
        .refresh_conversation_adapter_catalog()
        .expect("refresh adapter catalog");

    let projections = service
        .project_conversation_command_parts(
            crate::backend::conversations::ConversationCommandProjectionParams {
                adapter_id: "legacy-command-adapter".to_string(),
                parts: vec![
                    crate::backend::conversations::ConversationCommandProjectionPart {
                        part_id: "legacy-command-part".to_string(),
                        command: "printf '%s\\n' '--- tests ---' && pnpm test".to_string(),
                        command_label: None,
                    },
                ],
            },
        )
        .expect("project legacy adapter command through fallback");

    assert_eq!(projections.len(), 1);
    assert_eq!(projections[0].part_id, "legacy-command-part");
    assert_eq!(projections[0].nodes.len(), 1);
    assert_eq!(projections[0].nodes[0].command, "pnpm test");
    assert_eq!(
        projections[0].nodes[0].command_label.as_deref(),
        Some("tests")
    );

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn command_projection_falls_back_when_adapter_projector_is_unavailable() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-command-projector-unavailable-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create command projector test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let now = Utc::now().to_rfc3339();
    let unavailable_adapter = ConversationAdapter {
        id: "unavailable-command-adapter".to_string(),
        name: "Unavailable Command Adapter".to_string(),
        kind: ConversationAdapterKind::External,
        version: "1.0.0".to_string(),
        enabled: true,
        manifest_path: Some(
            root.join("missing-conversation-adapter.json")
                .to_string_lossy()
                .to_string(),
        ),
        executable_path: None,
        content_hash: None,
        trusted_hash: None,
        trust_state: ConversationAdapterTrustState::Trusted,
        protocol_version: Some(1),
        capabilities: vec![
            "read_session".to_string(),
            "project_command_parts".to_string(),
        ],
        input_kinds: vec![ConversationSourceKind::File],
        card_contract_version: None,
        card_kinds: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    };
    let pool = service.db.pool().clone();
    service
        .db
        .block_on(async move {
            crate::backend::store::upsert_conversation_adapter_sqlx(
                &pool,
                "default",
                &unavailable_adapter,
            )
            .await
        })
        .expect("save unavailable conversation adapter");
    service
        .runtime
        .refresh_conversation_adapter_catalog()
        .expect("refresh adapter catalog");

    let projections = service
        .project_conversation_command_parts(
            crate::backend::conversations::ConversationCommandProjectionParams {
                adapter_id: "unavailable-command-adapter".to_string(),
                parts: vec![
                    crate::backend::conversations::ConversationCommandProjectionPart {
                        part_id: "unavailable-command-part".to_string(),
                        command: "printf '%s\\n' '--- tests ---' && pnpm test".to_string(),
                        command_label: None,
                    },
                ],
            },
        )
        .expect("fall back when the adapter projector is unavailable");

    assert_eq!(projections.len(), 1);
    assert_eq!(projections[0].part_id, "unavailable-command-part");
    assert_eq!(projections[0].nodes[0].command, "pnpm test");
    assert_eq!(
        projections[0].nodes[0].command_label.as_deref(),
        Some("tests")
    );

    drop(service);
    fs::remove_dir_all(root).ok();
}

fn upsert_test_source(service: &AppService, source: &Source) {
    let pool = service.db.pool().clone();
    let tenant_id = service.tenant_id().to_string();
    service
        .db
        .block_on(async move {
            crate::backend::store::upsert_source_sqlx(&pool, &tenant_id, source).await
        })
        .expect("save source");
}

fn replace_test_source_assets(service: &AppService, source_id: &str, assets: &[Asset]) {
    let pool = service.db.pool().clone();
    let tenant_id = service.tenant_id().to_string();
    service
        .db
        .block_on(async move {
            crate::backend::store::replace_source_assets_sqlx(&pool, &tenant_id, source_id, assets)
                .await
        })
        .expect("save source assets");
}

fn load_test_assets(service: &AppService) -> Vec<Asset> {
    let pool = service.db.pool().clone();
    let tenant_id = service.tenant_id().to_string();
    service
        .db
        .block_on(
            async move { crate::backend::store::load_assets_sqlx(&pool, &tenant_id, None).await },
        )
        .expect("load assets")
}

#[test]
fn conversation_data_maintenance_audits_dry_runs_and_repairs_orphans_idempotently() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-maintenance-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create maintenance test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");

    let pool = service.db.pool().clone();
    service
        .db
        .block_on(async move {
            let mut connection = pool.acquire().await.map_err(AppError::external)?;
            sqlx::query("PRAGMA foreign_keys = OFF")
                .execute(&mut *connection)
                .await
                .map_err(AppError::external)?;
            sqlx::query(
                r#"
                INSERT INTO conversation_parts (
                    tenant_id, id, turn_id, part_index, role, kind, text, language,
                    command, cwd, status, exit_code, metadata_json, content_card_json,
                    translated_text, source_execution_id, command_label
                ) VALUES ('default', 'maintenance-orphan-part', 'maintenance-missing-turn', 0,
                    'assistant', 'text', 'orphan', NULL, NULL, NULL, NULL, NULL, NULL, NULL,
                    NULL, NULL, NULL)
                "#,
            )
            .execute(&mut *connection)
            .await
            .map_err(AppError::external)?;
            sqlx::query(
                r#"
                INSERT INTO conversation_question_turns (
                    tenant_id, question_id, turn_id, turn_order, assignment_origin,
                    assigned_at, updated_at
                ) VALUES ('default', 'maintenance-missing-question', 'maintenance-missing-turn', 0,
                    'imported', '2026-08-25T00:00:00Z', '2026-08-25T00:00:00Z')
                "#,
            )
            .execute(&mut *connection)
            .await
            .map_err(AppError::external)?;
            sqlx::query("PRAGMA foreign_keys = ON")
                .execute(&mut *connection)
                .await
                .map_err(AppError::external)?;
            AppResult::Ok(())
        })
        .expect("seed orphan conversation rows");

    let audit = service
        .audit_conversation_data(ConversationDataAuditParams {
            source_id: None,
            record_kind: Some("session".to_string()),
            include_resolved: false,
        })
        .expect("audit conversation data");
    assert!(audit["issue_count"].as_i64().unwrap_or_default() >= 2);
    assert!(audit["issues"]
        .as_array()
        .expect("audit issues")
        .iter()
        .any(|issue| issue["category"] == "orphan_parts"));
    assert!(audit["issues"]
        .as_array()
        .expect("audit issues")
        .iter()
        .any(|issue| issue["category"] == "orphan_memberships"));

    let dry_run = service
        .repair_conversation_data(ConversationDataRepairParams {
            record_kind: Some("session".to_string()),
            dry_run: true,
            yes: false,
            resync: false,
            ..ConversationDataRepairParams::default()
        })
        .expect("dry-run conversation repair");
    assert_eq!(dry_run["dry_run"], true);
    let orphan_count: i64 = service
        .db
        .block_on(async {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM conversation_parts WHERE id = 'maintenance-orphan-part'",
            )
            .fetch_one(service.db.pool())
            .await
            .map_err(AppError::external)
        })
        .expect("count orphan after dry-run");
    assert_eq!(orphan_count, 1);

    let resync_error = service.repair_conversation_data(ConversationDataRepairParams {
        source_id: Some("missing-source".to_string()),
        record_kind: Some("session".to_string()),
        yes: true,
        resync: true,
        ..ConversationDataRepairParams::default()
    });
    assert!(resync_error.is_err());
    let orphan_count_after_failed_resync: i64 = service
        .db
        .block_on(async {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM conversation_parts WHERE id = 'maintenance-orphan-part'",
            )
            .fetch_one(service.db.pool())
            .await
            .map_err(AppError::external)
        })
        .expect("count orphan after failed resync");
    assert_eq!(orphan_count_after_failed_resync, 1);

    let scoped_repair = service
        .repair_conversation_data(ConversationDataRepairParams {
            source_id: Some("unrelated-source".to_string()),
            record_kind: Some("session".to_string()),
            yes: true,
            resync: false,
            ..ConversationDataRepairParams::default()
        })
        .expect("apply source-scoped conversation repair");
    assert_eq!(scoped_repair["applied"]["deleted_parts"], 0);
    assert_eq!(scoped_repair["applied"]["deleted_memberships"], 0);
    assert!(!scoped_repair["audit"]["issues"]
        .as_array()
        .expect("scoped audit issues")
        .iter()
        .any(|issue| matches!(
            issue["category"].as_str(),
            Some("orphan_parts" | "orphan_memberships")
        )));
    let open_scoped_safe_issue_count: i64 = service
        .db
        .block_on(async {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM conversation_data_audit_issues WHERE tenant_id = 'default' AND status = 'open' AND auto_repairable = 1 AND fingerprint LIKE '%:source:unrelated-source'",
            )
            .fetch_one(service.db.pool())
            .await
            .map_err(AppError::external)
        })
        .expect("count open source-scoped safe audit issues");
    assert_eq!(open_scoped_safe_issue_count, 0);

    let repaired = service
        .repair_conversation_data(ConversationDataRepairParams {
            record_kind: Some("session".to_string()),
            yes: true,
            resync: false,
            ..ConversationDataRepairParams::default()
        })
        .expect("apply conversation repair");
    assert_eq!(repaired["dry_run"], false);
    assert_eq!(repaired["applied"]["deleted_parts"], 1);
    assert_eq!(repaired["applied"]["deleted_memberships"], 1);
    let backup_path = repaired["rollback"]["backup_path"]
        .as_str()
        .expect("repair rollback backup path");
    assert!(Path::new(backup_path).is_file());
    assert!(Path::new(backup_path).starts_with(root.join("conversation-repair-backups")));
    let resolved_issue_count: i64 = service
        .db
        .block_on(async {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM conversation_data_audit_issues WHERE tenant_id = 'default' AND status = 'resolved' AND category IN ('orphan_parts', 'orphan_memberships')",
            )
            .fetch_one(service.db.pool())
            .await
            .map_err(AppError::external)
        })
        .expect("count resolved audit issues");
    assert_eq!(resolved_issue_count, 2);
    let second = service
        .repair_conversation_data(ConversationDataRepairParams {
            record_kind: Some("session".to_string()),
            yes: true,
            resync: false,
            ..ConversationDataRepairParams::default()
        })
        .expect("repeat conversation repair");
    assert_eq!(second["applied"]["deleted_parts"], 0);
    assert_eq!(second["applied"]["deleted_memberships"], 0);

    let pool = service.db.pool().clone();
    service
        .db
        .block_on(async move {
            let mut connection = pool.acquire().await.map_err(AppError::external)?;
            sqlx::query("PRAGMA foreign_keys = OFF")
                .execute(&mut *connection)
                .await
                .map_err(AppError::external)?;
            sqlx::query(
                r#"
                INSERT INTO conversation_parts (
                    tenant_id, id, turn_id, part_index, role, kind, text, language,
                    command, cwd, status, exit_code, metadata_json, content_card_json,
                    translated_text, source_execution_id, command_label
                ) VALUES ('default', 'maintenance-recurrent-orphan-part',
                    'maintenance-recurrent-missing-turn', 0, 'assistant', 'text', 'orphan',
                    NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL)
                "#,
            )
            .execute(&mut *connection)
            .await
            .map_err(AppError::external)?;
            sqlx::query("PRAGMA foreign_keys = ON")
                .execute(&mut *connection)
                .await
                .map_err(AppError::external)?;
            AppResult::Ok(())
        })
        .expect("seed recurrent orphan conversation row");
    let recurrent = service
        .audit_conversation_data(ConversationDataAuditParams {
            record_kind: Some("session".to_string()),
            ..ConversationDataAuditParams::default()
        })
        .expect("audit a recurrent resolved issue");
    assert!(recurrent["issues"]
        .as_array()
        .expect("recurrent audit issues")
        .iter()
        .any(|issue| issue["category"] == "orphan_parts"));
    let recurrent_repair = service
        .repair_conversation_data(ConversationDataRepairParams {
            record_kind: Some("session".to_string()),
            yes: true,
            ..ConversationDataRepairParams::default()
        })
        .expect("repair a recurrent resolved issue");
    assert_eq!(recurrent_repair["applied"]["deleted_parts"], 1);

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn conversation_data_audit_reports_affected_snapshot_rows() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-audit-dependencies-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create snapshot audit test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");

    execute_test_sql(
        &service,
        r#"
        INSERT INTO conversation_data_audit_issues (
            tenant_id, id, category, fingerprint, severity, auto_repairable, status,
            affected_count, sample_ids_json, details_json, first_seen_at, last_seen_at
        ) VALUES (
            'default', 'question-snapshot-test', 'question_snapshot_dependencies',
            'question_snapshot_dependencies', 'warning', 0, 'open', 7, '[]', '{}',
            '2026-08-25T00:00:00Z', '2026-08-25T00:00:00Z'
        )
        "#,
    )
    .expect("seed snapshot audit fixture");

    let audit = service
        .audit_conversation_data(ConversationDataAuditParams::default())
        .expect("audit conversation dependencies");
    let issues = audit["issues"].as_array().expect("audit issues");
    let snapshot_issue = issues
        .iter()
        .find(|issue| issue["category"] == "question_snapshot_dependencies")
        .expect("snapshot dependency issue");
    assert_eq!(snapshot_issue["affected_count"], 7);
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn conversation_data_rollback_previews_requires_confirmation_and_restores_backup() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-rollback-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create rollback test root");
    let db_path = root.join("app.db");
    let backup_path = root.join("maintenance-backup.db");
    let service = AppService::open_with_db_path(db_path.clone()).expect("open rollback service");

    service
        .db
        .block_on(async {
            crate::backend::store::checkpoint_database_wal_sqlx(service.db.pool()).await
        })
        .expect("checkpoint database before backup");
    fs::copy(&db_path, &backup_path).expect("copy rollback fixture backup");

    execute_test_sql(
        &service,
        r#"
        INSERT INTO conversation_data_audit_issues (
            tenant_id, id, category, fingerprint, severity, status,
            first_seen_at, last_seen_at
        ) VALUES (
            'default', 'rollback-marker', 'search_index_mismatch',
            'rollback-marker', 'warning', 'open',
            '2026-08-25T00:00:00Z', '2026-08-25T00:00:00Z'
        )
        "#,
    )
    .expect("write post-backup marker");

    let preview = service
        .rollback_conversation_data(ConversationDataRollbackParams {
            backup_path: backup_path.to_string_lossy().into_owned(),
            dry_run: true,
            yes: false,
        })
        .expect("preview rollback");
    assert_eq!(preview["dry_run"], true);
    assert_eq!(preview["restored"], false);
    assert_eq!(preview["requires_app_restart"], true);

    let confirmation_error = service.rollback_conversation_data(ConversationDataRollbackParams {
        backup_path: backup_path.to_string_lossy().into_owned(),
        dry_run: false,
        yes: false,
    });
    assert!(confirmation_error.is_err());

    let restored = service
        .rollback_conversation_data(ConversationDataRollbackParams {
            backup_path: backup_path.to_string_lossy().into_owned(),
            dry_run: false,
            yes: true,
        })
        .expect("restore rollback backup");
    assert_eq!(restored["restored"], true);
    drop(service);

    let reopened = AppService::open_with_db_path(db_path).expect("reopen restored database");
    let marker_count: i64 = reopened
        .db
        .block_on(async {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM conversation_data_audit_issues WHERE id = 'rollback-marker'",
            )
            .fetch_one(reopened.db.pool())
            .await
            .map_err(AppError::external)
        })
        .expect("query restored database");
    assert_eq!(marker_count, 0);

    drop(reopened);
    fs::remove_dir_all(root).ok();
}

#[test]
fn creating_tenant_seeds_isolated_skill_backup_library_root() {
    let root = std::env::temp_dir().join(format!("assetiweave-tenant-root-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create temp dir");
    let db_path = root.join("app.db");

    let service = AppService::open_with_db_path(db_path.clone()).expect("open application service");
    let tenant = service
        .create_tenant(TenantCreateParams {
            name: "Client A".to_string(),
            slug: Some("client-a".to_string()),
            set_active: true,
        })
        .expect("create tenant");
    assert_eq!(tenant.id, "client-a");
    assert_eq!(tenant.slug, "client-a");
    assert_eq!(
        AppService::from_runtime(&service.runtime).tenant_id(),
        "client-a"
    );
    drop(service);

    let tenant_service =
        AppService::open_with_db_path(db_path).expect("open service for active tenant");
    assert_eq!(tenant_service.tenant_id(), "client-a");

    let settings = tenant_service
        .get_skill_backup_settings()
        .expect("load tenant skill backup settings");
    assert!(
        settings
            .expanded_root_path
            .replace('\\', "/")
            .ends_with(".assetiweave/tenants/client-a/library/skills"),
        "unexpected tenant skill root: {}",
        settings.expanded_root_path
    );
    assert_eq!(
        settings.default_root_path,
        "~/.assetiweave/tenants/client-a/library/skills"
    );
    assert!(settings.is_default_root);

    assert!(tenant_service
        .list_sources()
        .expect("list tenant sources")
        .iter()
        .any(|source| source.id == capabilities::SKILL_BACKUP_SOURCE_ID
            && source.root_path == settings.root_path));
    assert!(!tenant_service
        .list_profiles()
        .expect("list tenant profiles")
        .is_empty());
    let builtin_adapter_ids = tenant_service
        .list_conversation_adapters()
        .expect("list tenant conversation adapters")
        .into_iter()
        .filter(|adapter| adapter.trust_state == ConversationAdapterTrustState::BuiltIn)
        .map(|adapter| adapter.id)
        .collect::<std::collections::BTreeSet<_>>();
    let prepared_builtin_adapter_ids = tenant_service
        .runtime
        .builtin_conversation_adapters()
        .iter()
        .map(|adapter| adapter.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(!prepared_builtin_adapter_ids.is_empty());
    assert_eq!(builtin_adapter_ids, prepared_builtin_adapter_ids);

    drop(tenant_service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn switching_tenant_rebinds_the_next_app_service_request() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-tenant-switch-request-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create temp dir");
    let db_path = root.join("app.db");
    let source_root = root.join("tenant-b-source");
    fs::create_dir_all(&source_root).expect("create tenant B source root");

    let service = AppService::open_with_db_path(db_path).expect("open application service");
    let tenant_b = service
        .create_tenant(TenantCreateParams {
            name: "Tenant B".to_string(),
            slug: Some("tenant-b".to_string()),
            set_active: false,
        })
        .expect("create tenant B");
    let runtime_before_switch = service.runtime.context();

    service
        .switch_tenant(tenant_b.id.clone())
        .expect("switch to tenant B");
    let runtime_after_switch = service.runtime.context();
    assert!(std::sync::Arc::ptr_eq(
        &runtime_before_switch.agent_runtime_manager,
        &runtime_after_switch.agent_runtime_manager,
    ));
    assert!(std::sync::Arc::ptr_eq(
        &runtime_before_switch.agent_runtime,
        &runtime_after_switch.agent_runtime,
    ));

    let next_request = AppService::from_runtime(&service.runtime);
    assert_eq!(next_request.tenant_id(), "tenant-b");
    next_request
        .add_source(SourceInput {
            id: Some("tenant-b-source".to_string()),
            name: "Tenant B source".to_string(),
            kind: SourceKind::Local,
            root_path: source_root.to_string_lossy().to_string(),
            scanner_kind: None,
            source_origin: None,
            repo_root: None,
            scan_root: None,
            origin_app_kind: None,
            origin_provider_id: None,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            default_kind: None,
            enabled: true,
            priority: 0,
        })
        .expect("create source in tenant B");

    let pool = service.db.pool().clone();
    let (default_sources, tenant_b_sources) = service
        .db
        .block_on(async move {
            let default_sources =
                crate::backend::store::load_sources_sqlx(&pool, "default").await?;
            let tenant_b_sources =
                crate::backend::store::load_sources_sqlx(&pool, "tenant-b").await?;
            AppResult::Ok((default_sources, tenant_b_sources))
        })
        .expect("load sources by tenant");
    assert!(!default_sources
        .iter()
        .any(|source| source.id == "tenant-b-source"));
    assert!(tenant_b_sources
        .iter()
        .any(|source| source.id == "tenant-b-source"));

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn switching_tenant_rebinds_tenant_scoped_runtime_catalogs() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-tenant-switch-runtime-resources-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create temp dir");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let tenant_b = service
        .create_tenant(TenantCreateParams {
            name: "Tenant B".to_string(),
            slug: Some("tenant-b-runtime".to_string()),
            set_active: false,
        })
        .expect("create tenant B");
    execute_test_sql(
        &service,
        &format!(
            "INSERT INTO conversation_adapters (tenant_id, id, name, kind, version, enabled, manifest_path, executable_path, content_hash, trusted_hash, trust_state, protocol_version, capabilities, input_kinds, card_contract_version, card_kinds_json, created_at, updated_at) VALUES ('{}', 'tenant-b-only-adapter', 'Tenant B only adapter', 'external', '1.0.0', 1, NULL, NULL, NULL, NULL, 'trusted', 1, '[\"list\"]', '[\"directory\"]', NULL, '[]', '2026-08-23T00:00:00Z', '2026-08-23T00:00:00Z')",
            tenant_b.id
        ),
    )
    .expect("save tenant B adapter");

    service
        .switch_tenant(tenant_b.id)
        .expect("switch to tenant B");

    let next_request = AppService::from_runtime(&service.runtime);
    assert!(next_request
        .list_conversation_adapters()
        .expect("list tenant B adapters")
        .iter()
        .any(|adapter| adapter.id == "tenant-b-only-adapter"));

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn system_skill_source_cannot_be_edited_or_removed() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-system-source-protection-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let mut source =
        crate::backend::builtin_skills::system_skill_source().expect("build system Skill source");
    source.name = "Changed name".to_string();

    let update_error = service
        .update_source(source)
        .expect_err("system source update should fail");
    let remove_error = service
        .delete_source(crate::backend::builtin_skills::SYSTEM_SKILL_SOURCE_ID.to_string())
        .expect_err("system source removal should fail");

    assert!(update_error.to_string().contains("cannot be edited"));
    assert!(remove_error.to_string().contains("cannot be deleted"));

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn system_skill_cannot_be_copied_into_the_user_backup_library() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-system-skill-backup-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let source =
        crate::backend::builtin_skills::system_skill_source().expect("build system Skill source");
    let now = Utc::now().to_rfc3339();
    let asset = Asset {
        id: "system-skill-a".to_string(),
        source_id: source.id.clone(),
        name: "system-skill-a".to_string(),
        kind: AssetKind::Skill,
        detector_id: "legacy.classifier".to_string(),
        detector_version: 1,
        format: AssetFormat::Directory,
        relative_path: "system-skill-a".to_string(),
        absolute_path: root.join("system-skill-a").to_string_lossy().to_string(),
        entry_file: Some("SKILL.md".to_string()),
        description: None,
        content_hash: Some("system-skill-a-hash".to_string()),
        discovered_at: now.clone(),
        updated_at: now,
    };
    upsert_test_source(&service, &source);
    replace_test_source_assets(&service, &source.id, &[asset.clone()]);

    let error = service
        .backup_skill(asset.id)
        .expect_err("system Skill backup should fail");

    assert!(error.to_string().contains("cannot be backed up"));

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn doctor_reports_conversation_adapter_runtime_statuses() {
    let root = std::env::temp_dir().join(format!("assetiweave-doctor-runtime-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create temp dir");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");

    let report = service.run_doctor().expect("run doctor");
    let checks = report["checks"].as_array().expect("doctor checks");
    let runtime_check = checks
        .iter()
        .find(|check| check["name"] == "conversation_adapter_runtimes")
        .expect("runtime check");
    let details = runtime_check["details"]
        .as_array()
        .expect("runtime details");
    let kinds = details
        .iter()
        .filter_map(|detail| detail["kind"].as_str())
        .collect::<Vec<_>>();
    let node_available = details
        .iter()
        .find(|detail| detail["kind"].as_str() == Some("node"))
        .and_then(|detail| detail["available"].as_bool())
        .unwrap_or(false);
    let node_required_version = details
        .iter()
        .find(|detail| detail["kind"].as_str() == Some("node"))
        .and_then(|detail| detail["required_version"].as_str());

    assert_eq!(
        runtime_check["status"].as_str(),
        Some(if node_available { "pass" } else { "warn" })
    );
    assert_eq!(kinds, vec!["node", "python", "bash"]);
    assert_eq!(node_required_version, Some(">=20"));
    assert!(runtime_check["message"]
        .as_str()
        .expect("runtime message")
        .contains("runtimes available"));

    fs::remove_dir_all(root).ok();
}

#[test]
fn runtime_status_includes_harvester_runtime_requirements() {
    let root =
        std::env::temp_dir().join(format!("assetiweave-harvester-runtime-{}", Uuid::new_v4()));
    let source_dir = root.join("source");
    fs::create_dir_all(source_dir.join("scripts")).expect("create source dir");
    fs::write(source_dir.join("scripts").join("harvest.py"), "\n").expect("write harvester");
    fs::write(
        source_dir.join("harvester.json"),
        r#"{"schema_version":1,"id":"fixture-harvester","runtime":{"type":"python","entry":"scripts/harvest.py","version":">=3.12"}}"#,
    )
    .expect("write harvester manifest");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let source = ConversationSource {
        id: "fixture-harvester-source".to_string(),
        adapter_id: "codex".to_string(),
        name: "Fixture Harvester".to_string(),
        kind: ConversationSourceKind::Directory,
        location: source_dir.to_string_lossy().to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    };
    let pool = service.db.pool().clone();
    let tenant_id = service.tenant_id().to_string();
    service
        .db
        .block_on(async move {
            crate::backend::store::upsert_conversation_source_sqlx(&pool, &tenant_id, &source).await
        })
        .expect("save source");

    let statuses = service
        .list_conversation_adapter_runtime_statuses()
        .expect("list runtime statuses");
    let python_requirement = statuses
        .iter()
        .find(|status| {
            status.kind == crate::backend::conversations::ConversationAdapterRuntimeKind::Python
        })
        .and_then(|status| status.required_version.as_deref());

    assert_eq!(python_requirement, Some(">=3.12"));

    fs::remove_dir_all(root).ok();
}

fn set_test_asset_mount(
    service: &AppService,
    asset_id: &str,
    profile_id: &str,
    enabled: bool,
    strategy: DeploymentStrategy,
) {
    let pool = service.db.pool().clone();
    let tenant_id = service.tenant_id().to_string();
    service
        .db
        .block_on(async move {
            crate::backend::store::set_asset_mount_sqlx(
                &pool, &tenant_id, asset_id, profile_id, enabled, strategy,
            )
            .await
        })
        .expect("persist mount preference");
}

fn count_asset_rows(service: &AppService, table: &str, asset_id: &str) -> i64 {
    let pool = service.db.pool().clone();
    let statement = format!("SELECT COUNT(*) FROM {table} WHERE asset_id = ?");
    let asset_id = asset_id.to_string();
    service
        .db
        .block_on(async move {
            sqlx::query_scalar::<_, i64>(AssertSqlSafe(statement))
                .bind(asset_id)
                .fetch_one(&pool)
                .await
                .map_err(|error| error.to_string())
        })
        .expect("count asset rows")
}

#[cfg(unix)]
fn write_executable_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join(name);
    fs::write(&path, body).expect("write executable script");
    let mut permissions = fs::metadata(&path)
        .expect("read script metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("set script permissions");
    path
}

#[cfg(unix)]
fn adapter_manifest_entry(root: &Path, script: &Path) -> String {
    script
        .strip_prefix(root)
        .unwrap_or(script)
        .to_string_lossy()
        .to_string()
}

#[cfg(unix)]
fn upsert_conversation_export_fixture(
    service: &AppService,
    root: &Path,
    adapter_capabilities: Vec<String>,
    adapter_script: Option<&Path>,
    web_record: bool,
) -> String {
    let adapter_id = format!("fixture-export-{}", Uuid::new_v4());
    let source_id = format!("{adapter_id}-source");
    let manifest_path = root.join(format!("{adapter_id}.json"));
    if let Some(script) = adapter_script {
        fs::write(
            &manifest_path,
            serde_json::json!({
                "schema_version": 1,
                "id": &adapter_id,
                "name": "Fixture export adapter",
                "version": "0.1.0",
                "protocol_version": 1,
                "command": [adapter_manifest_entry(root, script)],
                "capabilities": &adapter_capabilities,
                "input_kinds": ["directory"]
            })
            .to_string(),
        )
        .expect("write export adapter manifest");
    } else {
        fs::write(&manifest_path, "{}").expect("write placeholder manifest");
    }
    let now = "2026-01-01T00:00:00Z".to_string();
    let adapter = ConversationAdapter {
        id: adapter_id.clone(),
        name: "Fixture export adapter".to_string(),
        kind: ConversationAdapterKind::External,
        version: "0.1.0".to_string(),
        enabled: true,
        manifest_path: Some(manifest_path.to_string_lossy().to_string()),
        executable_path: adapter_script.map(|path| path.to_string_lossy().to_string()),
        content_hash: None,
        trusted_hash: None,
        trust_state: ConversationAdapterTrustState::Trusted,
        protocol_version: Some(1),
        capabilities: adapter_capabilities,
        input_kinds: vec![ConversationSourceKind::Directory],
        card_contract_version: None,
        card_kinds: Vec::new(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let source = ConversationSource {
        id: source_id,
        adapter_id,
        name: "Fixture export source".to_string(),
        kind: ConversationSourceKind::Directory,
        location: root.to_string_lossy().to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: now.clone(),
        updated_at: now,
    };
    let session = NormalizedConversationSession {
        external_id: "export-session".to_string(),
        title: Some("Export Fixture".to_string()),
        project_path: Some(root.join("project").to_string_lossy().to_string()),
        started_at: None,
        updated_at: None,
        source_locator: None,
        source_fingerprint: None,
        turns: vec![NormalizedConversationTurn {
            external_id: "turn-1".to_string(),
            turn_index: 0,
            user_text: "Export this".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some("Rust fallback should not appear".to_string()),
                language: None,
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
                command_label: None,
                source_execution_id: None,
                content_card: None,
                metadata_json: Some(
                    r#"{"content_card":{"type":"answer","format":"markdown"}}"#.to_string(),
                ),
            }],
        }],
    };
    let pool = service.db.pool().clone();
    let tenant_id = service.tenant_id().to_string();
    let session_id = service
        .db
        .block_on(async move {
            crate::backend::store::upsert_conversation_adapter_sqlx(&pool, &tenant_id, &adapter)
                .await
                .map_err(AppError::external)?;
            crate::backend::store::upsert_conversation_source_sqlx(&pool, &tenant_id, &source)
                .await
                .map_err(AppError::external)?;
            let sessions = if web_record {
                crate::backend::store::import_web_record_sessions_sqlx(
                    &pool,
                    &tenant_id,
                    &source,
                    &[session],
                    false,
                )
                .await
                .map_err(AppError::external)?;
                crate::backend::store::list_web_record_sessions_sqlx(
                    &pool,
                    &tenant_id,
                    Some(&source.adapter_id),
                    Some(&source.id),
                    None,
                    1,
                    0,
                )
                .await?
            } else {
                crate::backend::store::import_conversation_sessions_sqlx(
                    &pool,
                    &tenant_id,
                    &source,
                    &[session],
                    false,
                )
                .await
                .map_err(AppError::external)?;
                crate::backend::store::list_conversation_sessions_sqlx(
                    &pool,
                    &tenant_id,
                    Some(&source.adapter_id),
                    Some(&source.id),
                    None,
                    1,
                    0,
                )
                .await
                .map_err(AppError::external)?
            };
            AppResult::Ok(sessions[0].session.id.clone())
        })
        .expect("upsert conversation export fixture");
    session_id
}

#[cfg(unix)]
fn load_export_fixture_adapter(service: &AppService, session_id: &str) -> ConversationAdapter {
    let pool = service.db.pool().clone();
    let tenant_id = service.tenant_id().to_string();
    let session_id = session_id.to_string();
    service
        .db
        .block_on(async move {
            let detail = crate::backend::store::load_conversation_session_detail_sqlx(
                &pool,
                &tenant_id,
                &session_id,
            )
            .await
            .map_err(|error| error.to_string())?;
            crate::backend::store::load_conversation_adapter_sqlx(
                &pool,
                &tenant_id,
                &detail.session.adapter_id,
            )
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "fixture adapter not found".to_string())
        })
        .expect("load export fixture adapter")
}

#[cfg(unix)]
#[test]
fn conversation_blocks_list_locators_and_get_selected_content_for_each_record_kind() {
    for (web_record, record_kind) in [(false, "session"), (true, "web")] {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-conversation-blocks-{record_kind}-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create conversation block fixture root");
        let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");
        let session_id =
            upsert_conversation_export_fixture(&service, &root, Vec::new(), None, web_record);
        let search = service
            .search_conversation_records(ConversationSearchParams {
                record_kind: Some(record_kind.to_string()),
                adapter_id: None,
                source_id: None,
                project_path: None,
                query: session_id,
                content_types: vec![crate::backend::dto::ConversationSearchCardType::question()],
                card_kinds: Vec::new(),
                semantic_roles: Vec::new(),
                include_questions: Some(true),
                include_cards: Some(false),
                since: None,
                until: None,
                timeline: false,
                limit: Some(5),
                offset: Some(0),
                search_options: None,
            })
            .expect("locate fixture question");
        let question_id = search.hits[0].question_id.clone();

        let blocks = service
            .list_conversation_blocks(ConversationBlockListParams { question_id })
            .expect("list block locators");
        assert_eq!(blocks.len(), 2);
        assert!(blocks.iter().all(|block| block.record_kind == record_kind));
        assert!(!serde_json::to_string(&blocks)
            .expect("serialize locators")
            .contains("Rust fallback should not appear"));

        let question_block = blocks
            .iter()
            .find(|block| block.kind == "question")
            .expect("question block");
        let answer_block = blocks
            .iter()
            .find(|block| block.semantic_role.as_deref() == Some("answer"))
            .expect("answer block");
        assert_eq!(question_block.content_length, "Export this".chars().count());
        assert_eq!(
            answer_block.content_length,
            "Rust fallback should not appear".chars().count()
        );

        let question = service
            .get_conversation_block(ConversationBlockGetParams {
                block_id: question_block.block_id.clone(),
            })
            .expect("load only the question block");
        assert_eq!(question.content, "Export this");

        let answer = service
            .get_conversation_block(ConversationBlockGetParams {
                block_id: answer_block.block_id.clone(),
            })
            .expect("load only the answer block");
        assert_eq!(answer.content, "Rust fallback should not appear");
        assert_eq!(answer.locator.kind, "answer");
        let _ = fs::remove_dir_all(root);
    }
}

#[cfg(unix)]
#[test]
fn conversation_session_export_uses_adapter_markdown_formatter() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-export-plugin-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let script = write_executable_script(
        &root,
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"item","item":{"kind":"markdown_export","content":"adapter markdown export","relative_path":"plugin/export.md"}}'
printf '%s\n' '{"type":"complete","item":{"export_count":1}}'
"#,
    );
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["export_markdown".to_string()],
        Some(&script),
        false,
    );
    let output_root = root.join("exports");

    let result = service
        .export_conversation_session(ConversationSessionExportParams {
            session_id,
            output_root: output_root.to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            format: crate::backend::dto::ConversationExportFormat::Rendered,
            dry_run: false,
        })
        .expect("export through adapter");

    let path = PathBuf::from(result["path"].as_str().expect("export path"));
    assert_eq!(path, output_root.join("plugin/export.md"));
    assert_eq!(
        fs::read_to_string(path).expect("read exported file"),
        "adapter markdown export"
    );
    assert_eq!(result["legacy_adapter_exporter_used"], true);
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn conversation_session_export_dry_run_calls_adapter_without_writing_file() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-export-dry-run-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let script = write_executable_script(
        &root,
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf 'ran' > "$0.ran"
printf '%s\n' '{"type":"item","item":{"kind":"markdown_export","content":"dry run markdown","relative_path":"plugin/dry-run.md"}}'
printf '%s\n' '{"type":"complete","item":{"export_count":1}}'
"#,
    );
    let ran_marker = script.with_file_name("adapter.sh.ran");
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["export_markdown".to_string()],
        Some(&script),
        false,
    );
    let output_root = root.join("exports");

    let result = service
        .export_conversation_session(ConversationSessionExportParams {
            session_id,
            output_root: output_root.to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            format: crate::backend::dto::ConversationExportFormat::Rendered,
            dry_run: true,
        })
        .expect("dry-run export through adapter");

    let path = PathBuf::from(result["path"].as_str().expect("export path"));
    assert_eq!(path, output_root.join("plugin/dry-run.md"));
    assert_eq!(result["bytes"], "dry run markdown".len());
    assert!(ran_marker.exists());
    assert!(!path.exists());
    assert_eq!(result["legacy_adapter_exporter_used"], true);
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn web_record_export_uses_adapter_markdown_formatter() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-web-record-export-plugin-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let script = write_executable_script(
        &root,
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"item","item":{"kind":"markdown_export","content":"web adapter markdown export","relative_path":"web/export.md"}}'
printf '%s\n' '{"type":"complete","item":{"export_count":1}}'
"#,
    );
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["export_markdown".to_string()],
        Some(&script),
        true,
    );
    let output_root = root.join("exports");

    let result = service
        .export_web_record_session(ConversationSessionExportParams {
            session_id,
            output_root: output_root.to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            format: crate::backend::dto::ConversationExportFormat::Rendered,
            dry_run: false,
        })
        .expect("export web record through adapter");

    let path = PathBuf::from(result["path"].as_str().expect("export path"));
    assert_eq!(path, output_root.join("web/export.md"));
    assert_eq!(
        fs::read_to_string(path).expect("read exported file"),
        "web adapter markdown export"
    );
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn conversation_session_export_falls_back_to_core_without_adapter_markdown_capability() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-export-no-cap-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["read_session".to_string()],
        None,
        false,
    );

    let result = service
        .export_conversation_session(ConversationSessionExportParams {
            session_id,
            output_root: root.join("exports").to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            format: crate::backend::dto::ConversationExportFormat::Rendered,
            dry_run: true,
        })
        .expect("Core exporter should handle adapters without export_markdown");

    assert_eq!(result["dry_run"], true);
    assert_eq!(result["written"], false);
    assert_eq!(result["legacy_adapter_exporter_used"], false);
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn conversation_raw_export_preserves_source_facts_and_excludes_question_snapshots() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-raw-export-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["read_session".to_string()],
        None,
        false,
    );
    let session_result = service
        .export_conversation_session(ConversationSessionExportParams {
            session_id,
            output_root: root.join("session-export").to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            format: crate::backend::dto::ConversationExportFormat::Raw,
            dry_run: false,
        })
        .expect("export raw session facts");
    let session_path = PathBuf::from(session_result["path"].as_str().expect("raw path"));
    let session_json: Value =
        serde_json::from_str(&fs::read_to_string(session_path).expect("read raw session export"))
            .expect("parse raw session export");
    assert_eq!(session_json["format"], "raw");
    assert_eq!(session_json["record_kind"], "session");
    assert_eq!(session_json["turns"][0]["user_text"], "Export this");
    assert_eq!(
        session_json["parts"][0]["text"],
        "Rust fallback should not appear"
    );
    assert!(session_json["questions"][0].get("question_text").is_none());
    assert!(session_json["questions"][0].get("answer_text").is_none());

    let web_session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["read_session".to_string()],
        None,
        true,
    );
    let web_result = service
        .export_web_record_session(ConversationSessionExportParams {
            session_id: web_session_id,
            output_root: root.join("web-export").to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            format: crate::backend::dto::ConversationExportFormat::Raw,
            dry_run: true,
        })
        .expect("preview raw web facts");
    assert_eq!(web_result["format"], "raw");
    assert_eq!(
        web_result["path"]
            .as_str()
            .unwrap_or_default()
            .rsplit_once('.')
            .map(|(_, ext)| ext),
        Some("json")
    );
    assert_eq!(web_result["legacy_adapter_exporter_used"], false);

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn card_contract_v1_export_uses_core_and_preserves_reasoning() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-export-v1-reasoning-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let script = write_executable_script(
        &root,
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf 'ran' > "$0.ran"
printf '%s\n' '{"type":"item","item":{"kind":"markdown_export","content":"legacy exporter","relative_path":"plugin/export.md"}}'
printf '%s\n' '{"type":"complete","item":{"export_count":1}}'
"#,
    );
    let marker = script.with_file_name("adapter.sh.ran");
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["export_markdown".to_string()],
        Some(&script),
        false,
    );
    let adapter = load_export_fixture_adapter(&service, &session_id);
    let pool = service.db.pool().clone();
    let tenant_id = service.tenant_id().to_string();
    service
        .db
        .block_on(async move {
            sqlx::query(
                "UPDATE conversation_adapters SET card_contract_version = 1, card_kinds_json = ?1 WHERE id = ?2",
            )
            .bind(r#"[{"id":"fixture-export.reasoning","semantic_role":"reasoning","label":"Reasoning","default_renderer":"markdown","allowed_renderers":["markdown"],"icon_hint":"brain"},{"id":"fixture-export.trace","semantic_role":"tool","label":"Trace","default_renderer":"json","allowed_renderers":["json"],"icon_hint":"braces"}]"#)
            .bind(&adapter.id)
            .execute(&pool)
            .await
            .map_err(AppError::external)?;
            sqlx::query(
                "INSERT INTO conversation_parts (tenant_id, id, turn_id, part_index, role, kind, text, language, command, cwd, status, exit_code, metadata_json, translated_text, content_card_json) SELECT tenant_id, 'fixture-json-part', turn_id, 1, role, 'text', '{\"step\":\"inspect\"}', NULL, NULL, NULL, NULL, NULL, NULL, NULL, '{\"schema_version\":1,\"kind\":\"fixture-export.trace\",\"renderer\":\"json\"}' FROM conversation_parts WHERE tenant_id = ?1 LIMIT 1",
            )
            .bind(&tenant_id)
            .execute(&pool)
            .await
            .map_err(AppError::external)?;
            sqlx::query(
                "INSERT INTO conversation_parts (tenant_id, id, turn_id, part_index, role, kind, text, language, command, cwd, status, exit_code, metadata_json, translated_text, content_card_json) SELECT tenant_id, 'fixture-history-part', turn_id, 2, role, 'text', 'Future history remains visible', NULL, NULL, NULL, NULL, NULL, NULL, NULL, '{\"schema_version\":1,\"kind\":\"future.history-note\",\"renderer\":\"plain\"}' FROM conversation_parts WHERE tenant_id = ?1 LIMIT 1",
            )
            .bind(&tenant_id)
            .execute(&pool)
            .await
            .map_err(AppError::external)?;
            sqlx::query(
                "UPDATE conversation_parts SET text = 'Compare both paths', metadata_json = '{\"source_type\":\"thinking\"}', content_card_json = '{\"schema_version\":1,\"kind\":\"fixture-export.reasoning\",\"renderer\":\"markdown\"}' WHERE tenant_id = ?1 AND id NOT IN ('fixture-json-part', 'fixture-history-part')",
            )
            .bind(tenant_id)
            .execute(&pool)
            .await
            .map_err(AppError::external)?;
            AppResult::Ok(())
        })
        .expect("promote fixture to Card Contract v1");
    let output_root = root.join("exports");

    let result = service
        .export_conversation_session(ConversationSessionExportParams {
            session_id,
            output_root: output_root.to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            format: crate::backend::dto::ConversationExportFormat::Rendered,
            dry_run: false,
        })
        .expect("export v1 reasoning through Core");

    let path = PathBuf::from(result["path"].as_str().expect("export path"));
    let markdown = fs::read_to_string(path).expect("read Core export");
    assert!(markdown.contains("### reasoning"));
    assert!(markdown.contains("Compare both paths"));
    assert!(markdown.contains("### trace"));
    assert!(markdown.contains("```json"));
    assert!(markdown.contains("\"step\":\"inspect\""));
    assert!(markdown.contains("### history note"));
    assert!(markdown.contains("Future history remains visible"));
    assert_eq!(result["legacy_adapter_exporter_used"], false);
    assert!(
        !marker.exists(),
        "v1 export must not invoke the legacy exporter"
    );
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn card_contract_v1_web_and_dry_run_exports_share_the_core_path() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-web-export-v1-reasoning-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");
    let script = write_executable_script(
        &root,
        "adapter.sh",
        "#!/bin/sh\ncat >/dev/null\nprintf 'ran' > \"$0.ran\"\n",
    );
    let marker = script.with_file_name("adapter.sh.ran");
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["export_markdown".to_string()],
        Some(&script),
        true,
    );
    let pool = service.db.pool().clone();
    let tenant_id = service.tenant_id().to_string();
    let lookup_session_id = session_id.clone();
    service
        .db
        .block_on(async move {
            let detail = crate::backend::store::load_web_record_session_detail_sqlx(
                &pool,
                &tenant_id,
                &lookup_session_id,
            )
            .await?;
            sqlx::query(
                "UPDATE conversation_adapters SET card_contract_version = 1, card_kinds_json = ?1 WHERE tenant_id = ?2 AND id = ?3",
            )
            .bind(r#"[{"id":"fixture-export.reasoning","semantic_role":"reasoning","label":"Reasoning","default_renderer":"markdown","allowed_renderers":["markdown"]}]"#)
            .bind(&tenant_id)
            .bind(&detail.session.adapter_id)
            .execute(&pool)
            .await
            .map_err(AppError::external)?;
            sqlx::query(
                "UPDATE web_record_parts SET text = 'Web reasoning survives', metadata_json = NULL, content_card_json = '{\"schema_version\":1,\"kind\":\"fixture-export.reasoning\",\"renderer\":\"markdown\"}' WHERE tenant_id = ?1",
            )
            .bind(tenant_id)
            .execute(&pool)
            .await
            .map_err(AppError::external)?;
            AppResult::Ok(())
        })
        .expect("promote web fixture to v1");
    let output_root = root.join("exports");

    let dry_run = service
        .export_web_record_session(ConversationSessionExportParams {
            session_id: session_id.clone(),
            output_root: output_root.to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            format: crate::backend::dto::ConversationExportFormat::Rendered,
            dry_run: true,
        })
        .expect("dry-run web export through Core");
    assert_eq!(dry_run["legacy_adapter_exporter_used"], false);
    assert!(!marker.exists());

    let written = service
        .export_web_record_session(ConversationSessionExportParams {
            session_id,
            output_root: output_root.to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            format: crate::backend::dto::ConversationExportFormat::Rendered,
            dry_run: false,
        })
        .expect("write web export through Core");
    let markdown = fs::read_to_string(written["path"].as_str().unwrap()).unwrap();
    assert!(markdown.contains("Web reasoning survives"));
    assert!(!marker.exists());
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn conversation_session_export_rejects_unsafe_adapter_relative_path() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-export-unsafe-path-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let script = write_executable_script(
        &root,
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"item","item":{"kind":"markdown_export","content":"unsafe","relative_path":"../escape.md"}}'
printf '%s\n' '{"type":"complete","item":{"export_count":1}}'
"#,
    );
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["export_markdown".to_string()],
        Some(&script),
        false,
    );

    let error = service
        .export_conversation_session(ConversationSessionExportParams {
            session_id,
            output_root: root.join("exports").to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            format: crate::backend::dto::ConversationExportFormat::Rendered,
            dry_run: true,
        })
        .expect_err("unsafe adapter relative path should fail");

    assert!(error.to_string().contains("relative_path"));
    assert!(!root.join("escape.md").exists());
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn conversation_session_export_requires_manifest_markdown_capability() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-export-manifest-no-cap-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let script = write_executable_script(
        &root,
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"item","item":{"kind":"markdown_export","content":"adapter markdown export","relative_path":"plugin/export.md"}}'
printf '%s\n' '{"type":"complete","item":{"export_count":1}}'
"#,
    );
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["export_markdown".to_string()],
        Some(&script),
        false,
    );
    let adapter = load_export_fixture_adapter(&service, &session_id);
    let manifest_path = adapter.manifest_path.expect("manifest path");
    fs::write(
        &manifest_path,
        serde_json::json!({
            "schema_version": 1,
            "id": adapter.id,
            "name": "Fixture export adapter",
            "version": "0.1.0",
            "protocol_version": 1,
            "command": [adapter_manifest_entry(&root, &script)],
            "capabilities": ["read_session"],
            "input_kinds": ["directory"]
        })
        .to_string(),
    )
    .expect("rewrite manifest without export capability");

    let error = service
        .export_conversation_session(ConversationSessionExportParams {
            session_id,
            output_root: root.join("exports").to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            format: crate::backend::dto::ConversationExportFormat::Rendered,
            dry_run: true,
        })
        .expect_err("manifest missing export_markdown should fail");

    assert!(error.to_string().contains("export_markdown"));
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn conversation_session_export_rejects_trusted_hash_mismatch() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-export-hash-mismatch-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let script = write_executable_script(
        &root,
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"item","item":{"kind":"markdown_export","content":"adapter markdown export","relative_path":"plugin/export.md"}}'
printf '%s\n' '{"type":"complete","item":{"export_count":1}}'
"#,
    );
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["export_markdown".to_string()],
        Some(&script),
        false,
    );
    let adapter = load_export_fixture_adapter(&service, &session_id);
    let pool = service.db.pool().clone();
    service
        .db
        .block_on(async move {
            sqlx::query("UPDATE conversation_adapters SET trusted_hash = ? WHERE id = ?")
                .bind("definitely-not-the-current-hash")
                .bind(&adapter.id)
                .execute(&pool)
                .await
                .map_err(AppError::external)?;
            AppResult::Ok(())
        })
        .expect("force hash mismatch");

    let error = service
        .export_conversation_session(ConversationSessionExportParams {
            session_id,
            output_root: root.join("exports").to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            format: crate::backend::dto::ConversationExportFormat::Rendered,
            dry_run: true,
        })
        .expect_err("trusted hash mismatch should fail");

    assert!(error.to_string().contains("trusted hash mismatch"));
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn conversation_session_export_rejects_manifest_tampering_after_trust() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-export-manifest-tamper-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let script = write_executable_script(
        &root,
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"item","item":{"kind":"markdown_export","content":"adapter markdown export","relative_path":"plugin/export.md"}}'
printf '%s\n' '{"type":"complete","item":{"export_count":1}}'
"#,
    );
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["export_markdown".to_string()],
        Some(&script),
        false,
    );
    let adapter = load_export_fixture_adapter(&service, &session_id);
    let manifest_path = adapter.manifest_path.clone().expect("manifest path");
    let validation = crate::backend::conversations::validate_external_adapter(
        crate::backend::conversations::ExternalAdapterValidateParams {
            manifest_path: manifest_path.clone(),
        },
    )
    .expect("validate adapter");
    let pool = service.db.pool().clone();
    let trusted_hash = validation.content_hash.clone();
    let adapter_id = adapter.id.clone();
    service
        .db
        .block_on(async move {
            sqlx::query(
                "UPDATE conversation_adapters SET content_hash = ?, trusted_hash = ? WHERE id = ?",
            )
            .bind(&trusted_hash)
            .bind(&trusted_hash)
            .bind(&adapter_id)
            .execute(&pool)
            .await
            .map_err(AppError::external)?;
            AppResult::Ok(())
        })
        .expect("store trusted hash");

    fs::write(
        &manifest_path,
        serde_json::json!({
            "schema_version": 1,
            "id": adapter.id,
            "name": "Fixture export adapter",
            "version": "0.1.0",
            "protocol_version": 1,
            "command": [adapter_manifest_entry(&root, &script), "--changed"],
            "capabilities": ["export_markdown"],
            "input_kinds": ["directory"]
        })
        .to_string(),
    )
    .expect("rewrite manifest with changed args");

    let error = service
        .export_conversation_session(ConversationSessionExportParams {
            session_id,
            output_root: root.join("exports").to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            format: crate::backend::dto::ConversationExportFormat::Rendered,
            dry_run: true,
        })
        .expect_err("manifest tampering should fail trusted hash check");

    assert!(error.to_string().contains("trusted hash mismatch"));
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn app_initialization_migrates_legacy_adapter_trusted_hashes() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-adapter-hash-migration-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let db_path = root.join("app.db");
    let script = write_executable_script(
        &root,
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"complete","item":{}}'
"#,
    );
    let manifest_path = root.join("conversation-adapter.json");
    fs::write(
        &manifest_path,
        serde_json::json!({
            "schema_version": 1,
            "id": "legacy-hash-adapter",
            "name": "Legacy Hash Adapter",
            "version": "0.1.0",
            "protocol_version": 1,
            "command": [adapter_manifest_entry(&root, &script)],
            "capabilities": ["probe", "read_session"],
            "input_kinds": ["directory"]
        })
        .to_string(),
    )
    .expect("write adapter manifest");
    let validation = crate::backend::conversations::validate_external_adapter(
        crate::backend::conversations::ExternalAdapterValidateParams {
            manifest_path: manifest_path.to_string_lossy().to_string(),
        },
    )
    .expect("validate adapter");
    let legacy_hash = validation.executable_hash.clone().expect("executable hash");
    {
        let database = crate::backend::store::Database::open(&db_path).expect("open raw database");
        let adapter = ConversationAdapter {
            id: "legacy-hash-adapter".to_string(),
            name: "Legacy Hash Adapter".to_string(),
            kind: ConversationAdapterKind::External,
            version: "0.1.0".to_string(),
            enabled: true,
            manifest_path: Some(manifest_path.to_string_lossy().to_string()),
            executable_path: Some(script.to_string_lossy().to_string()),
            content_hash: Some(legacy_hash.clone()),
            trusted_hash: Some(legacy_hash),
            trust_state: ConversationAdapterTrustState::Trusted,
            protocol_version: Some(1),
            capabilities: vec!["probe".to_string(), "read_session".to_string()],
            input_kinds: vec![ConversationSourceKind::Directory],
            card_contract_version: None,
            card_kinds: Vec::new(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let pool = database.pool().clone();
        let tenant_id = "default".to_string();
        database
            .block_on(async move {
                crate::backend::store::upsert_conversation_adapter_sqlx(&pool, &tenant_id, &adapter)
                    .await
            })
            .expect("insert legacy adapter");
    }

    let service = AppService::open_with_db_path(db_path).expect("open initialized service");
    let pool = service.db.pool().clone();
    let tenant_id = service.tenant_id().to_string();
    let migrated = service
        .db
        .block_on(async move {
            crate::backend::store::load_conversation_adapter_sqlx(
                &pool,
                &tenant_id,
                "legacy-hash-adapter",
            )
            .await
        })
        .expect("load migrated adapter")
        .expect("migrated adapter");

    assert_eq!(
        migrated.content_hash.as_deref(),
        Some(validation.content_hash.as_str())
    );
    assert_eq!(
        migrated.trusted_hash.as_deref(),
        Some(validation.content_hash.as_str())
    );
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn conversation_session_export_rejects_symlink_escape_under_output_root() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-export-symlink-escape-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let script = write_executable_script(
        &root,
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"item","item":{"kind":"markdown_export","content":"escape","relative_path":"link/export.md"}}'
printf '%s\n' '{"type":"complete","item":{"export_count":1}}'
"#,
    );
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["export_markdown".to_string()],
        Some(&script),
        false,
    );
    let output_root = root.join("exports");
    let outside_root = root.join("outside");
    fs::create_dir_all(&output_root).expect("create output root");
    fs::create_dir_all(&outside_root).expect("create outside root");
    symlink(&outside_root, output_root.join("link")).expect("create export symlink");

    let error = service
        .export_conversation_session(ConversationSessionExportParams {
            session_id,
            output_root: output_root.to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            format: crate::backend::dto::ConversationExportFormat::Rendered,
            dry_run: false,
        })
        .expect_err("symlink escape under output root should fail");

    assert!(error.to_string().contains("symlink") || error.to_string().contains("output_root"));
    assert!(!outside_root.join("export.md").exists());
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn navigation_model_updates_through_sqlx_path() {
    let root = std::env::temp_dir().join(format!("assetiweave-sqlx-navigation-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let mut model = service.navigation_model().expect("load navigation model");
    model.active_sub_nav_id = "sqlx-updated-sub-nav".to_string();
    model.rail_items[0].label = "SQLx Rail".to_string();

    let updated = service
        .update_navigation_model(model)
        .expect("update navigation model");

    assert_eq!(updated.active_sub_nav_id, "sqlx-updated-sub-nav");
    assert_eq!(updated.rail_items[0].label, "SQLx Rail");
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn app_shortcuts_update_through_sqlx_path() {
    let root = std::env::temp_dir().join(format!("assetiweave-sqlx-shortcuts-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let mut settings = service
        .list_app_shortcut_settings()
        .expect("load shortcut settings");
    settings[0].display_icon = "Q".to_string();
    settings[0].enabled = false;
    let disabled_profile_id = settings[0].profile_id.clone();

    let updated = service
        .update_app_shortcuts(settings)
        .expect("update shortcuts");
    let enabled = service
        .list_app_shortcuts()
        .expect("load enabled shortcuts");

    assert_eq!(updated[0].display_icon, "Q");
    assert!(!updated[0].enabled);
    assert!(enabled
        .iter()
        .all(|shortcut| shortcut.profile_id != disabled_profile_id));
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn profile_delete_guard_blocks_sqlx_deployment_state() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-sqlx-profile-delete-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let profile = service
        .create_profile(TargetProfileInput {
            id: Some("team-app".to_string()),
            name: "Team App".to_string(),
            app_kind: None,
            target_provider_id: None,
            target_paths: Some(vec![root.join("target").to_string_lossy().to_string()]),
            supported_kinds: None,
            deployment_strategy: None,
            enabled: Some(true),
            include: None,
            exclude: None,
            safety: None,
        })
        .expect("create profile");

    service
        .db
        .block_on(async {
            crate::backend::store::upsert_deployment_state_sqlx(
                service.db.pool(),
                service.tenant_id(),
                &DeploymentState {
                    profile_id: profile.id.clone(),
                    asset_id: "asset-a".to_string(),
                    target_path: "/target/a".to_string(),
                    strategy: DeploymentStrategy::SymlinkToSource,
                    source_hash: "hash".to_string(),
                    deployed_at: "2026-06-18T00:00:00Z".to_string(),
                    managed_by: "assetiweave".to_string(),
                },
            )
            .await
        })
        .expect("insert deployment state");

    let error = service
        .delete_profile(profile.id)
        .expect_err("delete blocked by deployment state");

    assert!(error.to_string().contains("managed deployments"));
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn scan_skill_sources_reads_sqlx_sources() {
    let root = std::env::temp_dir().join(format!("assetiweave-sqlx-scan-skill-{}", Uuid::new_v4()));
    let source_root = root.join("skills");
    let skill_dir = source_root.join("skill-a");
    fs::create_dir_all(&skill_dir).expect("create skill directory");
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: skill-a\n---\n\n# Skill A\n",
    )
    .expect("write skill file");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    clear_test_tables(&service, &["assets", "sources"]);
    service
        .add_source(SourceInput {
            id: Some("sqlx-skill-source".to_string()),
            name: "SQLx Skill Source".to_string(),
            kind: SourceKind::Local,
            root_path: source_root.to_string_lossy().to_string(),
            scanner_kind: Some(SourceScannerKind::Skill),
            source_origin: Some(SourceOrigin::LocalFolder),
            repo_root: None,
            scan_root: None,
            origin_app_kind: None,
            origin_provider_id: None,
            include_globs: vec!["**/SKILL.md".to_string()],
            exclude_globs: Vec::new(),
            default_kind: Some(AssetKind::Skill),
            enabled: true,
            priority: 0,
        })
        .expect("add source through service");

    let assets = service
        .scan_skill_sources()
        .expect("scan skill sources through service");

    assert!(assets
        .iter()
        .any(|candidate| candidate.asset.name == "skill-a"));
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn skill_group_crud_and_members_use_sqlx_path() {
    let root = std::env::temp_dir().join(format!("assetiweave-sqlx-groups-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    clear_test_tables(
        &service,
        &["asset_group_members", "asset_groups", "assets", "sources"],
    );

    let source = Source {
        id: "source-a".to_string(),
        name: "Source A".to_string(),
        kind: SourceKind::Local,
        root_path: root.join("source-a").to_string_lossy().to_string(),
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::LocalFolder,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        origin_provider_id: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: Vec::new(),
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    };
    let now = Utc::now().to_rfc3339();
    let assets = vec![
        Asset {
            id: "skill-a".to_string(),
            source_id: source.id.clone(),
            name: "Frontend UI".to_string(),
            kind: AssetKind::Skill,
            detector_id: "legacy.classifier".to_string(),
            detector_version: 1,
            format: AssetFormat::Directory,
            relative_path: "frontend/ui".to_string(),
            absolute_path: root
                .join("source-a/frontend/ui")
                .to_string_lossy()
                .to_string(),
            entry_file: Some("SKILL.md".to_string()),
            description: None,
            content_hash: Some("hash-a".to_string()),
            discovered_at: now.clone(),
            updated_at: now.clone(),
        },
        Asset {
            id: "skill-b".to_string(),
            source_id: source.id.clone(),
            name: "Backend API".to_string(),
            kind: AssetKind::Skill,
            detector_id: "legacy.classifier".to_string(),
            detector_version: 1,
            format: AssetFormat::Directory,
            relative_path: "backend/api".to_string(),
            absolute_path: root
                .join("source-a/backend/api")
                .to_string_lossy()
                .to_string(),
            entry_file: Some("SKILL.md".to_string()),
            description: None,
            content_hash: Some("hash-b".to_string()),
            discovered_at: now.clone(),
            updated_at: now,
        },
    ];
    let pool = service.db.pool().clone();
    let tenant_id = service.tenant_id().to_string();
    service
        .db
        .block_on(async move {
            crate::backend::store::upsert_source_sqlx(&pool, &tenant_id, &source).await?;
            crate::backend::store::replace_source_assets_sqlx(
                &pool, &tenant_id, "source-a", &assets,
            )
            .await
        })
        .expect("seed SQLx catalog");

    let created = service
        .create_skill_group(AssetGroupInput {
            id: Some("frontend".to_string()),
            name: "Frontend".to_string(),
            description: Some(" UI work ".to_string()),
            color: Some("#10b981".to_string()),
            display_icon: Some("F".to_string()),
            icon_svg: None,
            enabled: Some(true),
            sort_order: Some(1),
            rules: Some(AssetGroupRules {
                source_ids: vec!["source-a".to_string()],
                relative_path_globs: vec!["frontend/**".to_string()],
                name_contains: Some("ui".to_string()),
            }),
        })
        .expect("create SQLx group");
    assert_eq!(created.group.id, "frontend");
    assert_eq!(created.members.len(), 1);
    assert_eq!(created.members[0].asset_id, "skill-a");

    let with_manual = service
        .set_skill_group_manual_members(
            "frontend".to_string(),
            vec!["skill-b".to_string(), "skill-b".to_string()],
        )
        .expect("save SQLx manual members");
    assert_eq!(with_manual.manual_asset_ids, vec!["skill-b".to_string()]);
    assert_eq!(with_manual.members.len(), 2);

    let mut updated_group = with_manual.group.clone();
    updated_group.name = "Frontend Updated".to_string();
    let updated = service
        .update_skill_group(updated_group)
        .expect("update SQLx group");
    assert_eq!(updated.group.name, "Frontend Updated");
    assert_eq!(
        service
            .get_skill_group("frontend".to_string())
            .expect("get SQLx group")
            .group
            .name,
        "Frontend Updated"
    );
    assert_eq!(
        service.list_skill_groups().expect("list SQLx groups").len(),
        1
    );

    service
        .delete_skill_group("frontend".to_string())
        .expect("delete SQLx group");
    assert!(service
        .list_skill_groups()
        .expect("list after delete")
        .is_empty());

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn cleanup_orphan_asset_records_uses_sqlx_for_migrated_tables() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-sqlx-orphan-cleanup-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    execute_test_sql(
        &service,
        r#"
                INSERT INTO asset_mounts (
                    asset_id, profile_id, enabled, strategy, created_at, updated_at
                ) VALUES (
                    'orphan-asset', 'codex', 1, 'symlink_to_source',
                    '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
                );
                INSERT INTO deployment_state (
                    profile_id, asset_id, target_path, strategy,
                    source_hash, deployed_at, managed_by
                ) VALUES (
                    'codex', 'orphan-asset', '/tmp/orphan-asset', 'symlink_to_source',
                    'hash', '2026-01-01T00:00:00Z', 'assetiweave'
                );
                INSERT INTO skill_remote_sources (
                    asset_id, provider, source_url, repo_url, branch,
                    acquired_at, status
                ) VALUES (
                    'orphan-asset', 'github',
                    'https://github.com/example/repo/tree/main/skill',
                    'https://github.com/example/repo.git',
                    'main', '2026-01-01T00:00:00Z', 'unknown'
                );
                INSERT INTO asset_groups (
                    id, name, color, asset_kind, enabled, sort_order,
                    rules_payload, created_at, updated_at
                ) VALUES (
                    'orphan-group', 'Orphan Group', '#10b981', 'skill', 1, 0,
                    '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
                );
                INSERT INTO asset_group_members (group_id, asset_id, created_at)
                VALUES ('orphan-group', 'orphan-asset', '2026-01-01T00:00:00Z');
                "#,
    )
    .expect("seed orphan records");

    capabilities::cleanup_orphan_asset_records(&service.db, service.tenant_id())
        .expect("cleanup orphan records");

    for table in [
        "asset_mounts",
        "deployment_state",
        "skill_remote_sources",
        "asset_group_members",
    ] {
        let count = count_asset_rows(&service, table, "orphan-asset");
        assert_eq!(count, 0, "orphan row remained in {table}");
    }

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn list_skill_remote_sources_prunes_orphans_through_sqlx_path() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-sqlx-skill-remote-cleanup-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let orphan = SkillRemoteSource {
        asset_id: "missing-asset".to_string(),
        provider: "github".to_string(),
        source_url: "https://github.com/example/repo/tree/main/skill".to_string(),
        repo_url: "https://github.com/example/repo.git".to_string(),
        branch: "main".to_string(),
        path: Some("skill".to_string()),
        acquired_at: "2026-01-01T00:00:00Z".to_string(),
        acquired_tree_sha: None,
        local_content_hash: None,
        last_checked_at: None,
        latest_tree_sha: None,
        status: "unknown".to_string(),
        message: None,
    };
    let pool = service.db.pool().clone();
    let tenant_id = service.tenant_id().to_string();
    service
        .db
        .block_on(async move {
            crate::backend::store::upsert_skill_remote_source_sqlx(&pool, &tenant_id, &orphan).await
        })
        .expect("save orphan remote source");

    assert!(service
        .list_skill_remote_sources()
        .expect("list remote sources")
        .is_empty());

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn disabled_mount_preference_persists_through_sqlx_path() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-sqlx-disabled-mount-preference-{}",
        Uuid::new_v4()
    ));
    let source_root = root.join("source");
    let target_root = root.join("target");
    let skill_root = source_root.join("skill-a");
    fs::create_dir_all(&skill_root).expect("create skill source");
    fs::create_dir_all(&target_root).expect("create target root");
    fs::write(
        skill_root.join("SKILL.md"),
        "---\ndescription: Skill A\n---\n",
    )
    .expect("write skill");

    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    clear_test_tables(
        &service,
        &["asset_mounts", "deployment_state", "assets", "sources"],
    );
    let source = Source {
        id: "source-a".to_string(),
        name: "Source A".to_string(),
        kind: SourceKind::Local,
        root_path: source_root.to_string_lossy().to_string(),
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::LocalFolder,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        origin_provider_id: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: Vec::new(),
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    };
    upsert_test_source(&service, &source);

    let now = Utc::now().to_rfc3339();
    let asset = Asset {
        id: "asset-a".to_string(),
        source_id: source.id.clone(),
        name: "skill-a".to_string(),
        kind: AssetKind::Skill,
        detector_id: "legacy.classifier".to_string(),
        detector_version: 1,
        format: AssetFormat::Directory,
        relative_path: "skill-a".to_string(),
        absolute_path: skill_root.to_string_lossy().to_string(),
        entry_file: Some(skill_root.join("SKILL.md").to_string_lossy().to_string()),
        description: None,
        content_hash: Some("hash-a".to_string()),
        discovered_at: now.clone(),
        updated_at: now,
    };
    replace_test_source_assets(&service, &source.id, &[asset.clone()]);

    let profile = service
        .create_profile(TargetProfileInput {
            id: Some("target-a".to_string()),
            name: "Target A".to_string(),
            app_kind: Some(crate::backend::models::AppKind::Custom),
            target_provider_id: None,
            target_paths: Some(vec![target_root.to_string_lossy().to_string()]),
            supported_kinds: None,
            deployment_strategy: Some(DeploymentStrategy::SymlinkToSource),
            enabled: Some(true),
            include: None,
            exclude: None,
            safety: None,
        })
        .expect("create target profile");

    let mount = service
        .set_asset_mount(
            &asset.id,
            &profile.id,
            false,
            Some(DeploymentStrategy::CopyToTarget),
        )
        .expect("persist disabled preference");
    assert!(!mount.enabled);
    assert_eq!(mount.strategy, DeploymentStrategy::CopyToTarget);

    let saved_mounts = service
        .list_asset_mounts(Some(&asset.id))
        .expect("read SQLx mount preference");
    assert_eq!(saved_mounts, vec![mount]);

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn mount_skill_dry_run_reads_profile_through_sqlx_path() {
    let root =
        std::env::temp_dir().join(format!("assetiweave-sqlx-mount-dry-run-{}", Uuid::new_v4()));
    let source_root = root.join("source");
    let target_root = root.join("target");
    let skill_root = source_root.join("skill-a");
    fs::create_dir_all(&skill_root).expect("create skill source");
    fs::create_dir_all(&target_root).expect("create target root");
    fs::write(
        skill_root.join("SKILL.md"),
        "---\ndescription: Skill A\n---\n",
    )
    .expect("write skill");

    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    clear_test_tables(
        &service,
        &["asset_mounts", "deployment_state", "assets", "sources"],
    );
    let source = Source {
        id: "source-a".to_string(),
        name: "Source A".to_string(),
        kind: SourceKind::Local,
        root_path: source_root.to_string_lossy().to_string(),
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::LocalFolder,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        origin_provider_id: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: Vec::new(),
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    };
    upsert_test_source(&service, &source);

    let now = Utc::now().to_rfc3339();
    let asset = Asset {
        id: "asset-a".to_string(),
        source_id: source.id.clone(),
        name: "skill-a".to_string(),
        kind: AssetKind::Skill,
        detector_id: "legacy.classifier".to_string(),
        detector_version: 1,
        format: AssetFormat::Directory,
        relative_path: "skill-a".to_string(),
        absolute_path: skill_root.to_string_lossy().to_string(),
        entry_file: Some(skill_root.join("SKILL.md").to_string_lossy().to_string()),
        description: None,
        content_hash: Some("hash-a".to_string()),
        discovered_at: now.clone(),
        updated_at: now,
    };
    replace_test_source_assets(&service, &source.id, &[asset.clone()]);

    let profile = service
        .create_profile(TargetProfileInput {
            id: Some("target-a".to_string()),
            name: "Target A".to_string(),
            app_kind: Some(crate::backend::models::AppKind::Custom),
            target_provider_id: None,
            target_paths: Some(vec![target_root.to_string_lossy().to_string()]),
            supported_kinds: None,
            deployment_strategy: Some(DeploymentStrategy::SymlinkToSource),
            enabled: Some(true),
            include: None,
            exclude: None,
            safety: None,
        })
        .expect("create target profile");

    let preview = service
        .mount_skill(
            AssetRefParams {
                asset_ref: asset.id.clone(),
                profile_id: Some(profile.id.clone()),
                dry_run: true,
                yes: false,
                unmount: false,
            },
            true,
        )
        .expect("dry-run mount skill");

    assert_eq!(preview["dry_run"], json!(true));
    assert_eq!(preview["profile_id"], json!(profile.id));
    assert_eq!(preview["status"]["state"], json!("not_mounted"));
    assert!(!target_root.join("skill-a").exists());
    assert!(service
        .list_asset_mounts(Some(&asset.id))
        .expect("load mounts after dry-run")
        .is_empty());

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn batch_skill_backup_deduplicates_assets_and_reports_copy_progress() {
    let root =
        std::env::temp_dir().join(format!("assetiweave-skill-backup-batch-{}", Uuid::new_v4()));
    let source_root = root.join("source");
    let backup_root = root.join("backup");
    fs::create_dir_all(source_root.join("skill-a")).expect("create first skill");
    fs::create_dir_all(source_root.join("skill-b")).expect("create second skill");
    fs::write(
        source_root.join("skill-a").join("SKILL.md"),
        "---\ndescription: Skill A\n---\n",
    )
    .expect("write first skill");
    fs::write(
        source_root.join("skill-b").join("SKILL.md"),
        "---\ndescription: Skill B\n---\n",
    )
    .expect("write second skill");

    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    clear_test_tables(&service, &["assets", "sources"]);
    let source = Source {
        id: "source-a".to_string(),
        name: "Source A".to_string(),
        kind: SourceKind::Local,
        root_path: source_root.to_string_lossy().to_string(),
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::LocalFolder,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        origin_provider_id: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: Vec::new(),
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    };
    upsert_test_source(&service, &source);
    service
        .update_skill_backup_settings(UpdateSkillBackupSettingsParams {
            root_path: backup_root.to_string_lossy().to_string(),
            migrate: false,
        })
        .expect("configure backup root");

    let mut source_assets = load_test_assets(&service)
        .into_iter()
        .filter(|asset| asset.source_id == "source-a")
        .collect::<Vec<_>>();
    source_assets.sort_by(|left, right| left.name.cmp(&right.name));
    assert_eq!(source_assets.len(), 2);

    let first_id = source_assets[0].id.clone();
    let second_id = source_assets[1].id.clone();
    let mut progress = Vec::new();
    let backed_up = service
        .backup_skills_with_progress(
            vec![first_id.clone(), first_id, second_id.clone()],
            |completed, next_asset_id| {
                progress.push((completed, next_asset_id.map(str::to_string)));
            },
        )
        .expect("back up skills");

    assert_eq!(backed_up.len(), 2);
    assert_eq!(progress, vec![(1, Some(second_id)), (2, None)]);
    assert!(backup_root
        .join("backed-up")
        .join("source-a")
        .join("skill-a")
        .join("SKILL.md")
        .is_file());
    assert!(backup_root
        .join("backed-up")
        .join("source-a")
        .join("skill-b")
        .join("SKILL.md")
        .is_file());
    assert!(backed_up.iter().all(|asset| asset.backup_status.is_some()));

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn backed_up_duplicate_skill_is_hidden_from_plan_and_mount_statuses() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-skill-backup-duplicate-plan-{}",
        Uuid::new_v4()
    ));
    let source_root = root.join("source");
    let backup_root = root.join("backup");
    let target_root = root.join("target");
    fs::create_dir_all(source_root.join("skill-a")).expect("create skill");
    fs::create_dir_all(&target_root).expect("create target root");
    fs::write(
        source_root.join("skill-a").join("SKILL.md"),
        "---\ndescription: Skill A\n---\n",
    )
    .expect("write skill");

    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    clear_test_tables(&service, &["assets", "sources"]);
    let source = Source {
        id: "source-a".to_string(),
        name: "Source A".to_string(),
        kind: SourceKind::Local,
        root_path: source_root.to_string_lossy().to_string(),
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::LocalFolder,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        origin_provider_id: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: Vec::new(),
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    };
    upsert_test_source(&service, &source);
    let profile = service
        .create_profile(TargetProfileInput {
            id: Some("test-target".to_string()),
            name: "Test Target".to_string(),
            app_kind: Some(crate::backend::models::AppKind::Custom),
            target_provider_id: None,
            target_paths: Some(vec![target_root.to_string_lossy().to_string()]),
            supported_kinds: None,
            deployment_strategy: Some(DeploymentStrategy::SymlinkToSource),
            enabled: Some(true),
            include: None,
            exclude: None,
            safety: None,
        })
        .expect("create target profile");
    service
        .update_skill_backup_settings(UpdateSkillBackupSettingsParams {
            root_path: backup_root.to_string_lossy().to_string(),
            migrate: false,
        })
        .expect("configure backup root");

    let source_asset = load_test_assets(&service)
        .into_iter()
        .find(|asset| asset.source_id == "source-a")
        .expect("source asset");
    service
        .backup_skill(source_asset.id.clone())
        .expect("backup skill");

    let raw_skill_assets = load_test_assets(&service)
        .into_iter()
        .filter(|asset| asset.kind == AssetKind::Skill)
        .collect::<Vec<_>>();
    assert_eq!(raw_skill_assets.len(), 2);
    for asset in &raw_skill_assets {
        set_test_asset_mount(
            &service,
            &asset.id,
            &profile.id,
            true,
            DeploymentStrategy::SymlinkToSource,
        );
    }

    let catalog = service.list_skills().expect("list catalog");
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].asset.source_id, "source-a");
    assert_eq!(
        catalog[0]
            .backup_status
            .as_ref()
            .map(|status| status.hidden_asset_ids.len()),
        Some(1)
    );

    let plan = service
        .create_plan(Some(&profile.id))
        .expect("create deployment plan");
    assert_eq!(plan.actions.len(), 1);
    assert_eq!(
        plan.actions[0].asset_id.as_deref(),
        Some(source_asset.id.as_str())
    );
    assert_eq!(plan.summary.create_count, 1);
    assert_eq!(plan.summary.conflict_count, 0);

    let target_statuses = service
        .list_asset_mount_statuses(None)
        .expect("list mount statuses")
        .into_iter()
        .filter(|status| status.profile_id == profile.id)
        .collect::<Vec<_>>();
    assert_eq!(target_statuses.len(), 1);
    assert_eq!(target_statuses[0].asset_id, source_asset.id);

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn deleted_backup_library_copy_clears_source_backup_status() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-skill-backup-deleted-status-{}",
        Uuid::new_v4()
    ));
    let source_root = root.join("source");
    let backup_root = root.join("backup");
    fs::create_dir_all(source_root.join("skill-a")).expect("create skill");
    fs::write(
        source_root.join("skill-a").join("SKILL.md"),
        "---\ndescription: Skill A\n---\n",
    )
    .expect("write skill");

    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    clear_test_tables(&service, &["assets", "sources"]);
    let source = Source {
        id: "source-a".to_string(),
        name: "Source A".to_string(),
        kind: SourceKind::Local,
        root_path: source_root.to_string_lossy().to_string(),
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::LocalFolder,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        origin_provider_id: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: Vec::new(),
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    };
    upsert_test_source(&service, &source);
    service
        .update_skill_backup_settings(UpdateSkillBackupSettingsParams {
            root_path: backup_root.to_string_lossy().to_string(),
            migrate: false,
        })
        .expect("configure backup root");

    let source_asset = load_test_assets(&service)
        .into_iter()
        .find(|asset| asset.source_id == "source-a")
        .expect("source asset");
    let backed_up = service
        .backup_skill(source_asset.id.clone())
        .expect("backup skill");
    let backup_path = backed_up
        .backup_status
        .as_ref()
        .and_then(|status| status.backup_path.as_deref())
        .expect("backup path");
    fs::remove_dir_all(backup_path).expect("delete backup copy outside app");

    let catalog = service.list_skills().expect("list catalog");
    let source_catalog_asset = catalog
        .iter()
        .find(|candidate| candidate.asset.id == source_asset.id)
        .expect("source catalog asset");
    assert_eq!(source_catalog_asset.asset.source_id, "source-a");
    assert!(source_catalog_asset.backup_status.is_none());

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn stale_backup_record_outside_current_root_does_not_mark_git_skill_backed_up() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-stale-backup-root-status-{}",
        Uuid::new_v4()
    ));
    let source_root = root.join("source-repo");
    let source_skill_path = source_root.join("skills").join("canvas-design");
    let current_backup_root = root.join("current-backup");
    let stale_backup_root = root.join("old-backup");
    let stale_backup_path = stale_backup_root
        .join("backed-up")
        .join("source-a")
        .join("canvas-design");
    fs::create_dir_all(&source_skill_path).expect("create source skill");
    fs::create_dir_all(&current_backup_root).expect("create current backup root");
    fs::create_dir_all(&stale_backup_path).expect("create stale backup skill");
    fs::write(
        source_skill_path.join("SKILL.md"),
        "---\ndescription: Canvas design\n---\n",
    )
    .expect("write source skill");
    fs::write(
        stale_backup_path.join("SKILL.md"),
        "---\ndescription: Canvas design\n---\n",
    )
    .expect("write stale backup skill");

    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    clear_test_tables(&service, &["assets", "sources"]);
    let source = Source {
        id: "source-a".to_string(),
        name: "Git Source".to_string(),
        kind: SourceKind::Local,
        root_path: source_root.to_string_lossy().to_string(),
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::GitRepo,
        repo_root: Some(source_root.to_string_lossy().to_string()),
        scan_root: String::new(),
        origin_app_kind: None,
        origin_provider_id: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: Vec::new(),
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    };
    let backup_source = capabilities::assetiweave_library_source_with_root(
        current_backup_root.to_string_lossy().to_string(),
    );
    upsert_test_source(&service, &source);
    upsert_test_source(&service, &backup_source);

    let source_asset = Asset {
        id: "source-a-canvas-design".to_string(),
        source_id: source.id.clone(),
        name: "canvas-design".to_string(),
        kind: AssetKind::Skill,
        detector_id: "legacy.classifier".to_string(),
        detector_version: 1,
        format: AssetFormat::Directory,
        relative_path: "skills/canvas-design".to_string(),
        absolute_path: source_skill_path.to_string_lossy().to_string(),
        entry_file: Some("skills/canvas-design/SKILL.md".to_string()),
        description: Some("Canvas design".to_string()),
        content_hash: Some("same-content".to_string()),
        discovered_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    };
    let stale_backup_asset = Asset {
        id: "backup-canvas-design".to_string(),
        source_id: backup_source.id.clone(),
        name: "canvas-design".to_string(),
        kind: AssetKind::Skill,
        detector_id: "legacy.classifier".to_string(),
        detector_version: 1,
        format: AssetFormat::Directory,
        relative_path: "backed-up/source-a/canvas-design".to_string(),
        absolute_path: stale_backup_path.to_string_lossy().to_string(),
        entry_file: Some("backed-up/source-a/canvas-design/SKILL.md".to_string()),
        description: Some("Canvas design".to_string()),
        content_hash: Some("same-content".to_string()),
        discovered_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    };
    replace_test_source_assets(&service, &source.id, std::slice::from_ref(&source_asset));
    replace_test_source_assets(
        &service,
        &backup_source.id,
        std::slice::from_ref(&stale_backup_asset),
    );

    let catalog = service.list_skills().expect("list catalog");
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].asset.id, source_asset.id);
    assert!(catalog[0].backup_status.is_none());

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn app_target_backup_copy_does_not_report_identical_target_as_conflict() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-app-target-backup-status-{}",
        Uuid::new_v4()
    ));
    let app_target_root = root.join("codex-skills");
    let backup_root = root.join("backup");
    let skill_path = app_target_root.join("browser-testing-with-devtools");
    fs::create_dir_all(&skill_path).expect("create app target skill");
    fs::write(
        skill_path.join("SKILL.md"),
        "---\ndescription: Browser testing\n---\n",
    )
    .expect("write skill");

    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    clear_test_tables(&service, &["assets", "sources"]);
    let source = Source {
        id: "codex-source".to_string(),
        name: "Codex Source".to_string(),
        kind: SourceKind::Local,
        root_path: app_target_root.to_string_lossy().to_string(),
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::AppTarget,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: Some(crate::backend::models::AppKind::Codex),
        origin_provider_id: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: Vec::new(),
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    };
    upsert_test_source(&service, &source);
    let profile = service
        .create_profile(TargetProfileInput {
            id: Some("codex-test".to_string()),
            name: "Codex Test".to_string(),
            app_kind: Some(crate::backend::models::AppKind::Codex),
            target_provider_id: None,
            target_paths: Some(vec![app_target_root.to_string_lossy().to_string()]),
            supported_kinds: None,
            deployment_strategy: Some(DeploymentStrategy::SymlinkToSource),
            enabled: Some(true),
            include: None,
            exclude: None,
            safety: None,
        })
        .expect("create codex target profile");
    service
        .update_skill_backup_settings(UpdateSkillBackupSettingsParams {
            root_path: backup_root.to_string_lossy().to_string(),
            migrate: false,
        })
        .expect("configure backup root");

    let app_asset = load_test_assets(&service)
        .into_iter()
        .find(|asset| asset.source_id == "codex-source")
        .expect("app target asset");
    service
        .backup_skill(app_asset.id)
        .expect("backup app target skill");

    let catalog = service.list_skills().expect("list catalog");
    assert_eq!(catalog.len(), 1);
    assert_eq!(
        catalog[0].asset.source_id,
        capabilities::SKILL_BACKUP_SOURCE_ID
    );

    let statuses = service
        .list_asset_mount_statuses(None)
        .expect("list mount statuses");
    let status = statuses
        .iter()
        .find(|status| status.profile_id == profile.id)
        .expect("status for profile");
    assert_eq!(status.asset_id, catalog[0].asset.id);
    assert_eq!(status.state, PhysicalMountStateDto::NotMounted);

    let plan = service
        .create_plan(Some(&profile.id))
        .expect("create deployment plan");
    assert_eq!(plan.summary.conflict_count, 0);

    let mounted = service
        .mount_asset_by_id(&catalog[0].asset.id, &profile.id)
        .expect("mount backup copy over identical app target");
    assert_eq!(mounted.status.state, PhysicalMountStateDto::Mounted);
    let target_metadata = fs::symlink_metadata(&skill_path).expect("target metadata");
    assert!(target_metadata.file_type().is_symlink());
    assert_eq!(
        fs::read_link(&skill_path)
            .expect("read target symlink")
            .canonicalize()
            .expect("canonical target symlink"),
        PathBuf::from(&catalog[0].asset.absolute_path)
            .canonicalize()
            .expect("canonical backup path")
    );

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn refreshing_target_catalog_reconciles_existing_default_profiles() {
    let root =
        std::env::temp_dir().join(format!("assetiweave-target-reconcile-{}", Uuid::new_v4()));
    let skill_target = root.join("codex-skills");
    let prompt_target = root.join("codex-prompts");
    fs::create_dir_all(&root).expect("create target reconciliation root");
    let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");

    service
        .runtime
        .refresh_target_catalog(vec![crate::backend::models::TargetProfileDescriptor {
            id: "codex".to_string(),
            name: "Codex Fixture".to_string(),
            app_kind_compat: Some(crate::backend::models::AppKind::Codex),
            default_targets: vec![
                crate::backend::models::TargetPathRule {
                    asset_kind: AssetKind::Skill,
                    path: skill_target.to_string_lossy().to_string(),
                },
                crate::backend::models::TargetPathRule {
                    asset_kind: AssetKind::Prompt,
                    path: prompt_target.to_string_lossy().to_string(),
                },
            ],
            supported_kinds: vec![AssetKind::Skill, AssetKind::Prompt],
            deployment_strategy: DeploymentStrategy::SymlinkToSource,
            icon: None,
        }])
        .expect("refresh target catalog");

    let profile = service
        .list_profiles()
        .expect("list profiles")
        .into_iter()
        .find(|profile| profile.id == "codex")
        .expect("existing Codex profile");
    assert_eq!(
        profile.target_paths,
        vec![
            skill_target.to_string_lossy().to_string(),
            prompt_target.to_string_lossy().to_string(),
        ]
    );

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn injected_target_catalog_drives_seed_detect_plan_and_mount() {
    let root = std::env::temp_dir().join(format!("assetiweave-target-runtime-{}", Uuid::new_v4()));
    let source_root = root.join("fixture-source");
    let target_root = root.join("fixture-target");
    fs::create_dir_all(&source_root).expect("create fixture source");
    fs::create_dir_all(&target_root).expect("create fixture target");
    let source_file = source_root.join("SKILL.md");
    fs::write(&source_file, "---\ndescription: fixture\n---\n").expect("write fixture asset");

    let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");
    let descriptor = crate::backend::models::TargetProfileDescriptor {
        id: "fixture-provider".to_string(),
        name: "Fixture Provider".to_string(),
        app_kind_compat: None,
        default_targets: vec![crate::backend::models::TargetPathRule {
            asset_kind: AssetKind::Skill,
            path: target_root.to_string_lossy().to_string(),
        }],
        supported_kinds: vec![AssetKind::Skill],
        deployment_strategy: DeploymentStrategy::SymlinkToSource,
        icon: None,
    };
    service
        .runtime
        .refresh_target_catalog(vec![descriptor])
        .expect("publish fixture target catalog");

    service
        .create_tenant(TenantCreateParams {
            name: "Fixture tenant".to_string(),
            slug: Some("fixture-tenant".to_string()),
            set_active: true,
        })
        .expect("seed fixture provider profile");
    let service = AppService::from_runtime(&service.runtime);
    assert!(service
        .list_profiles()
        .expect("list seeded profiles")
        .iter()
        .any(|profile| profile.target_provider_id == "fixture-provider"));

    let detected = service
        .add_source(SourceInput {
            id: Some("fixture-detect-source".to_string()),
            name: "Fixture target source".to_string(),
            kind: crate::backend::models::SourceKind::Local,
            root_path: target_root.to_string_lossy().to_string(),
            scanner_kind: Some(crate::backend::models::SourceScannerKind::Skill),
            source_origin: Some(crate::backend::models::SourceOrigin::LocalFolder),
            repo_root: None,
            scan_root: None,
            origin_app_kind: None,
            origin_provider_id: None,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            default_kind: Some(AssetKind::Skill),
            enabled: true,
            priority: 0,
        })
        .expect("save detected target source");
    assert_eq!(
        detected.origin_provider_id.as_deref(),
        Some("fixture-provider")
    );

    let source = Source {
        id: "fixture-asset-source".to_string(),
        name: "Fixture asset source".to_string(),
        kind: crate::backend::models::SourceKind::Local,
        root_path: source_root.to_string_lossy().to_string(),
        scanner_kind: crate::backend::models::SourceScannerKind::Skill,
        source_origin: crate::backend::models::SourceOrigin::LocalFolder,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        origin_provider_id: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: Vec::new(),
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    };
    upsert_test_source(&service, &source);
    replace_test_source_assets(
        &service,
        &source.id,
        &[Asset {
            id: "fixture-asset".to_string(),
            source_id: source.id.clone(),
            name: "Fixture Skill".to_string(),
            kind: AssetKind::Skill,
            detector_id: "fixture.detector".to_string(),
            detector_version: 1,
            format: crate::backend::models::AssetFormat::Markdown,
            relative_path: "SKILL.md".to_string(),
            absolute_path: source_file.to_string_lossy().to_string(),
            entry_file: Some("SKILL.md".to_string()),
            description: Some("fixture".to_string()),
            content_hash: None,
            discovered_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        }],
    );

    let profile = service
        .list_profiles()
        .expect("list fixture profiles")
        .into_iter()
        .find(|profile| profile.target_provider_id == "fixture-provider")
        .expect("fixture provider profile");
    execute_test_sql(
        &service,
        &format!(
            "INSERT INTO asset_mounts (tenant_id, asset_id, profile_id, enabled, strategy, created_at, updated_at) VALUES ('{}', 'fixture-asset', '{}', 1, 'symlink_to_source', '2026-08-21T00:00:00Z', '2026-08-21T00:00:00Z')",
            service.tenant_id(),
            profile.id,
        ),
    )
    .expect("record fixture mount intent");
    let plan = service
        .create_plan(Some(&profile.id))
        .expect("build plan from injected catalog");
    assert_eq!(plan.summary.create_count, 1);
    let execution = service
        .execute_plan(plan, None)
        .expect("execute plan from injected catalog");
    assert_eq!(execution.executed_count, 1);
    assert!(target_root.join("Fixture Skill.md").is_symlink());

    let invalid = service.runtime.refresh_target_catalog(vec![
        crate::backend::models::TargetProfileDescriptor {
            id: "fixture-provider".to_string(),
            name: "Fixture Provider".to_string(),
            app_kind_compat: None,
            default_targets: vec![crate::backend::models::TargetPathRule {
                asset_kind: AssetKind::Skill,
                path: String::new(),
            }],
            supported_kinds: vec![AssetKind::Skill],
            deployment_strategy: DeploymentStrategy::SymlinkToSource,
            icon: None,
        },
    ]);
    assert!(invalid.is_err());
    assert!(service
        .runtime
        .target_catalog()
        .descriptor("fixture-provider")
        .is_some());

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn invalid_disk_target_catalog_refresh_preserves_the_published_snapshot() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-target-disk-refresh-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(root.join("target-providers")).expect("create provider directory");
    let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");
    assert!(service
        .list_target_profile_descriptors()
        .expect("list initial descriptors")
        .iter()
        .any(|descriptor| descriptor.id == "codex"));
    fs::write(
        root.join("target-providers/invalid.json"),
        serde_json::json!({
            "id": "invalid-provider",
            "name": "Invalid Provider",
            "app_kind_compat": null,
            "default_targets": [{ "asset_kind": "skill", "path": "" }],
            "supported_kinds": ["skill"],
            "deployment_strategy": "symlink_to_source",
            "icon": null
        })
        .to_string(),
    )
    .expect("write invalid descriptor");

    assert!(service.refresh_target_profile_descriptors().is_err());
    assert!(service
        .list_target_profile_descriptors()
        .expect("list preserved descriptors")
        .iter()
        .any(|descriptor| descriptor.id == "codex"));
    assert!(!service
        .list_target_profile_descriptors()
        .expect("list preserved descriptors")
        .iter()
        .any(|descriptor| descriptor.id == "invalid-provider"));

    drop(service);
    fs::remove_dir_all(root).ok();
}

fn github_repo_item() -> Value {
    json!({
        "full_name": "util6/util6-agents",
        "html_url": "https://github.com/util6/util6-agents",
        "clone_url": "https://github.com/util6/util6-agents.git",
        "default_branch": "main",
        "description": "Codex skills and agent workflows",
        "stargazers_count": 7
    })
}

fn github_code_item() -> Value {
    json!({
        "path": "skills/browser/SKILL.md",
        "repository": {
            "full_name": "util6/util6-agents",
            "html_url": "https://github.com/util6/util6-agents",
            "clone_url": "https://github.com/util6/util6-agents.git",
            "default_branch": "main",
            "description": "Codex skills and agent workflows",
            "stargazers_count": 7
        }
    })
}

#[test]
fn skill_search_provider_supports_github_code_aliases() {
    assert_eq!(
        normalize_skill_search_provider(None).as_deref().ok(),
        Some("github")
    );
    assert_eq!(
        normalize_skill_search_provider(Some("github_code"))
            .as_deref()
            .ok(),
        Some("github-code")
    );
    assert_eq!(
        normalize_skill_search_provider(Some("code"))
            .as_deref()
            .ok(),
        Some("github-code")
    );
    assert!(normalize_skill_search_provider(Some("unknown")).is_err());
}

#[test]
fn github_code_search_url_targets_skill_markdown_files() {
    let url = github_code_search_url("browser automation", 25);

    assert_eq!(
        url,
        "https://api.github.com/search/code?q=browser+automation+filename%3ASKILL.md&per_page=20"
    );
}

#[test]
fn github_tree_paths_extract_concrete_skill_dirs() {
    let value = json!({
        "tree": [
            { "path": "SKILL.md", "type": "blob" },
            { "path": "skills/browser/SKILL.md", "type": "blob" },
            { "path": "skills/browser/README.md", "type": "blob" },
            { "path": "skills/../escape/SKILL.md", "type": "blob" },
            { "path": "plugins/browser/SKILL.md", "type": "tree" }
        ]
    });

    let paths = github_skill_paths_from_tree_value(&value);

    assert_eq!(paths, vec!["".to_string(), "skills/browser".to_string()]);
}

#[test]
fn github_tree_sha_for_skill_path_reads_root_and_nested_tree() {
    let value = json!({
        "sha": "root-tree",
        "tree": [
            { "path": "skills/browser", "type": "tree", "sha": "browser-tree" },
            { "path": "skills/browser/SKILL.md", "type": "blob", "sha": "skill-file" }
        ]
    });

    assert_eq!(
        github_tree_sha_for_skill_path(&value, None).as_deref().ok(),
        Some("root-tree")
    );
    assert_eq!(
        github_tree_sha_for_skill_path(&value, Some("skills/browser"))
            .as_deref()
            .ok(),
        Some("browser-tree")
    );
    assert!(github_tree_sha_for_skill_path(&value, Some("missing")).is_err());
}

#[test]
fn github_skill_path_candidate_points_acquire_at_tree_url() {
    let repo_candidate = skill_search_candidate_from_github(&github_repo_item()).unwrap();

    let candidate = skill_search_candidate_from_github_skill_path(
        &repo_candidate,
        "util6/util6-agents",
        "main",
        "skills/browser",
    );

    assert_eq!(candidate.name, "util6/util6-agents/skills/browser");
    assert_eq!(candidate.path.as_deref(), Some("skills/browser"));
    assert_eq!(
        candidate.match_reason.as_deref(),
        Some("Resolved concrete Skill directory from skills/browser/SKILL.md")
    );
    assert_eq!(
        candidate.url,
        "https://github.com/util6/util6-agents/tree/main/skills/browser"
    );
    assert_eq!(
            candidate.acquire_command,
            "assetiweave-cli skill acquire --url https://github.com/util6/util6-agents/tree/main/skills/browser --yes"
        );
}

#[test]
fn github_code_candidate_points_acquire_at_skill_directory() {
    let candidate = skill_search_candidate_from_github_code(&github_code_item())
        .expect("github code item should become candidate");

    assert_eq!(candidate.name, "util6/util6-agents/skills/browser");
    assert_eq!(candidate.path.as_deref(), Some("skills/browser"));
    assert_eq!(
        candidate.match_reason.as_deref(),
        Some("GitHub code search matched skills/browser/SKILL.md")
    );
    assert_eq!(
        candidate.url,
        "https://github.com/util6/util6-agents/tree/main/skills/browser"
    );
    assert_eq!(
            candidate.acquire_command,
            "assetiweave-cli skill acquire --url https://github.com/util6/util6-agents/tree/main/skills/browser --yes"
        );
}

#[test]
fn root_skill_path_candidate_keeps_repo_name() {
    let repo_candidate = skill_search_candidate_from_github(&github_repo_item()).unwrap();

    let candidate = skill_search_candidate_from_github_skill_path(
        &repo_candidate,
        "util6/util6-agents",
        "main",
        "",
    );

    assert_eq!(candidate.name, "util6/util6-agents");
    assert_eq!(candidate.path, None);
    assert_eq!(
        candidate.match_reason.as_deref(),
        Some("Resolved concrete Skill directory from SKILL.md")
    );
    assert_eq!(
        candidate.url,
        "https://github.com/util6/util6-agents/tree/main"
    );
}

#[test]
fn repository_fallback_candidate_explains_missing_skill_path() {
    let repo_candidate = skill_search_candidate_from_github(&github_repo_item()).unwrap();

    let candidate = skill_search_repository_fallback_candidate(repo_candidate, "main");

    assert_eq!(
        candidate.match_reason.as_deref(),
        Some("Repository fallback: no concrete SKILL.md directory was resolved on branch main")
    );
    assert_eq!(
        candidate.acquire_command,
        "assetiweave-cli skill acquire --url https://github.com/util6/util6-agents --yes"
    );
}

#[test]
fn concrete_skill_candidate_scores_above_repo_fallback() {
    let repo_candidate = skill_search_candidate_from_github(&github_repo_item()).unwrap();
    let skill_candidate = skill_search_candidate_from_github_skill_path(
        &repo_candidate,
        "util6/util6-agents",
        "main",
        "skills/browser",
    );
    let terms = search_query_terms("browser skill");

    assert!(
        skill_candidate_score(&skill_candidate, &terms)
            > skill_candidate_score(&repo_candidate, &terms)
    );
}
#[test]
fn conversation_search_index_status_reports_missing_lexical_index() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-search-index-status-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create temp search index status directory");
    let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");

    let status = service
        .get_conversation_search_index_status()
        .expect("load conversation search index status");

    assert_eq!(status.health, "missing");
    assert_eq!(status.source_revision, 0);
    assert_eq!(status.indexed_revision, None);
    assert_eq!(
        status.supported_modes,
        vec![crate::backend::dto::SearchRetrievalMode::Lexical]
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn conversation_search_index_rebuild_publishes_a_ready_generation() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-search-index-rebuild-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create temp search index rebuild directory");
    let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");

    let report = service
        .rebuild_conversation_search_index()
        .expect("rebuild conversation search index");
    let status = service
        .get_conversation_search_index_status()
        .expect("load rebuilt conversation search index status");

    assert_eq!(report.document_count, 0);
    assert_eq!(report.indexed_revision, 0);
    assert!(!report.generation.is_empty());
    assert_eq!(status.health, "ready");
    assert_eq!(
        status.active_generation.as_deref(),
        Some(report.generation.as_str())
    );
    assert_eq!(status.indexed_revision, Some(status.source_revision));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn conversation_search_index_rebuild_failure_releases_writer_lease() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-search-index-failure-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create temp search index failure directory");
    let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");
    fs::write(root.join("conversation-search-index"), "not a directory")
        .expect("block search index directory");

    assert!(service.rebuild_conversation_search_index().is_err());
    let status = service
        .get_conversation_search_index_status()
        .expect("load failed conversation search index status");
    assert_eq!(status.health, "failed");
    assert!(!status.is_rebuilding);
    assert!(status.lease_owner.is_none());
    assert!(status.last_error.is_some());
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn conversation_search_uses_ready_tantivy_index_and_hydrates_sqlite_records() {
    let root =
        std::env::temp_dir().join(format!("assetiweave-search-index-query-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create temp search index query directory");
    let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");
    upsert_conversation_export_fixture(&service, &root, vec![], None, false);
    let stale_legacy = service
        .search_conversation_records(ConversationSearchParams {
            record_kind: Some("session".to_string()),
            adapter_id: None,
            source_id: None,
            project_path: None,
            query: "stale-answer-snapshot".to_string(),
            content_types: vec![crate::backend::dto::ConversationSearchCardType::answer()],
            card_kinds: Vec::new(),
            semantic_roles: Vec::new(),
            include_questions: None,
            include_cards: None,
            since: None,
            until: None,
            timeline: false,
            limit: Some(20),
            offset: Some(0),
            search_options: None,
        })
        .expect("search without stale question snapshot fields");
    assert_eq!(stale_legacy.total_count, 0);
    assert!(service
        .list_conversation_sessions(ConversationSessionListParams {
            adapter_id: None,
            source_id: None,
            query: Some("stale-answer-snapshot".to_string()),
            limit: Some(20),
            offset: Some(0),
        })
        .expect("list sessions without stale question snapshot fields")
        .is_empty());
    let legacy = service
        .search_conversation_records(ConversationSearchParams {
            record_kind: Some("session".to_string()),
            adapter_id: None,
            source_id: None,
            project_path: None,
            query: "Rust fallback".to_string(),
            content_types: vec![crate::backend::dto::ConversationSearchCardType::answer()],
            card_kinds: Vec::new(),
            semantic_roles: Vec::new(),
            include_questions: None,
            include_cards: None,
            since: None,
            until: None,
            timeline: false,
            limit: Some(20),
            offset: Some(0),
            search_options: None,
        })
        .expect("search SQLite projection before the derived index exists");
    assert_eq!(legacy.backend, "legacy_scan");
    assert_eq!(legacy.total_count, 1);
    assert_eq!(legacy.hits[0].question_title, "Export this");
    let report = service
        .rebuild_conversation_search_index()
        .expect("rebuild conversation search index");
    assert_eq!(report.document_count, 2);

    let result = service
        .search_conversation_records(ConversationSearchParams {
            record_kind: Some("session".to_string()),
            adapter_id: None,
            source_id: None,
            project_path: None,
            query: "Rust fallback".to_string(),
            content_types: vec![crate::backend::dto::ConversationSearchCardType::answer()],
            card_kinds: Vec::new(),
            semantic_roles: Vec::new(),
            include_questions: None,
            include_cards: None,
            since: None,
            until: None,
            timeline: false,
            limit: Some(20),
            offset: Some(0),
            search_options: None,
        })
        .expect("search rebuilt conversation index");

    assert_eq!(result.backend, "tantivy");
    assert_eq!(result.total_count, 1);
    assert_eq!(result.hits[0].part_id, legacy.hits[0].part_id);
    assert_eq!(result.hits[0].card_type, legacy.hits[0].card_type);
    assert_eq!(result.hits[0].question_title, "Export this");
    assert_eq!(result.hits[0].snippet, legacy.hits[0].snippet);
    assert_eq!(
        result.hits[0].card_type,
        crate::backend::dto::ConversationSearchCardType::answer()
    );
    assert!(result.hits[0].snippet.contains("Rust fallback"));
    assert_eq!(
        result
            .content_type_counts
            .as_ref()
            .and_then(|counts| counts.get("answer")),
        Some(&1)
    );
    assert!(result.hits[0]
        .highlight_segments
        .as_ref()
        .is_some_and(|segments| segments.iter().any(|segment| segment.matched)));

    let detail = service
        .get_conversation_question(ConversationQuestionGetParams {
            question_id: result.hits[0].question_id.clone(),
        })
        .expect("load the same persisted Part through the detail DTO");
    assert_eq!(detail.question_turns.len(), 1);
    assert_eq!(detail.question_turns[0].turn_id, detail.turns[0].id);
    assert_eq!(detail.question_turns[0].turn_order, 0);
    let part_id = result.hits[0].part_id.as_deref().expect("answer Part id");
    let node = detail
        .projected_content_nodes
        .iter()
        .find(|node| node.part_id == part_id)
        .expect("detail Content Node for indexed Part");
    assert_eq!(node.node_type, result.hits[0].card_type.as_str());
    assert_eq!(node.semantic_role.as_deref(), Some("answer"));
    assert_eq!(node.content, result.hits[0].snippet);
    let memory_card = super::memory_recall::recall_card_projection_for_test(&detail)
        .into_iter()
        .find(|(candidate_part_id, _, _)| candidate_part_id == part_id)
        .expect("Memory evidence for the same Part");
    assert_eq!(memory_card.1, node.node_type);
    assert_eq!(memory_card.2, node.content);
    assert_eq!(
        result
            .semantic_role_counts
            .as_ref()
            .and_then(|counts| counts.get("answer")),
        Some(&1)
    );

    let second_turn_id = format!("app-membership-turn-{}", Uuid::new_v4());
    let second_question_id = format!("app-membership-question-{}", Uuid::new_v4());
    let now = "2026-08-25T00:00:00Z";
    let pool = service.db.pool().clone();
    let tenant_id = service.tenant_id().to_string();
    let session_id = detail.question.session_id.clone();
    let question_id = detail.question.id.clone();
    let second_turn_id_for_db = second_turn_id.clone();
    let second_question_id_for_db = second_question_id.clone();
    service
        .db
        .block_on(async move {
            sqlx::query(
                r#"
                INSERT INTO conversation_turns (
                    tenant_id, id, session_id, external_id, turn_index, user_text, title,
                    started_at, ended_at, fingerprint, missing, imported_at
                )
                VALUES (?1, ?2, ?3, ?4, 1, ?5, NULL, NULL, NULL, ?6, 0, ?7)
                "#,
            )
            .bind(&tenant_id)
            .bind(&second_turn_id_for_db)
            .bind(&session_id)
            .bind("app-membership-turn-2")
            .bind("Second prompt")
            .bind("app-membership-fingerprint")
            .bind(now)
            .execute(&pool)
            .await
            .map_err(AppError::external)?;
            sqlx::query(
                r#"
                INSERT INTO conversation_questions (
                    tenant_id, id, session_id, title, created_at, updated_at
                )
                VALUES (?1, ?2, ?3, NULL, ?4, ?4)
                "#,
            )
            .bind(&tenant_id)
            .bind(&second_question_id_for_db)
            .bind(&session_id)
            .bind(now)
            .execute(&pool)
            .await
            .map_err(AppError::external)?;
            sqlx::query(
                r#"
                INSERT INTO conversation_question_turns (
                    tenant_id, question_id, turn_id, turn_order,
                    assignment_origin, assigned_at, updated_at
                )
                VALUES (?1, ?2, ?3, 0, 'imported', ?4, ?4)
                "#,
            )
            .bind(&tenant_id)
            .bind(&second_question_id_for_db)
            .bind(&second_turn_id_for_db)
            .bind(now)
            .execute(&pool)
            .await
            .map_err(AppError::external)?;
            Ok::<_, AppError>(())
        })
        .expect("seed a second question for AppService mutation");

    let merged = service
        .merge_conversation_questions(ConversationQuestionMergeParams {
            question_ids: vec![question_id.clone(), second_question_id],
            dry_run: false,
        })
        .expect("merge question memberships through AppService");
    assert_eq!(merged.questions.len(), 1);
    assert_eq!(merged.questions[0].question.id, question_id);
    assert_eq!(merged.questions[0].question_turns.len(), 2);
    assert_eq!(
        merged.questions[0].question_turns[1].turn_id,
        second_turn_id
    );
    assert_eq!(
        merged.questions[0].question_turns[1].assignment_origin,
        ConversationGroupingOrigin::Manual
    );
    assert_eq!(
        merged.questions[0].question.title.as_deref(),
        Some("Export this")
    );

    let session_fragment =
        crate::backend::models::conversation_id_fragment(&result.hits[0].session.session.id);
    let id_result = service
        .search_conversation_records(ConversationSearchParams {
            record_kind: Some("session".to_string()),
            adapter_id: None,
            source_id: None,
            project_path: None,
            query: session_fragment,
            content_types: vec![crate::backend::dto::ConversationSearchCardType::answer()],
            card_kinds: Vec::new(),
            semantic_roles: Vec::new(),
            include_questions: None,
            include_cards: None,
            since: None,
            until: None,
            timeline: false,
            limit: Some(20),
            offset: Some(0),
            search_options: None,
        })
        .expect("search rebuilt conversation index by session id fragment");
    assert_eq!(id_result.backend, "id_lookup");
    assert_eq!(id_result.total_count, 1);
    assert!(id_result.hits[0].snippet.starts_with("Rust fallback"));
    assert!(id_result.hits[0].highlight_segments.is_none());

    service
        .update_conversation_part_translation(ConversationPartTranslationUpdateParams {
            record_kind: Some("session".to_string()),
            part_id: result.hits[0].part_id.clone().expect("answer part id"),
            translated_text: "Rust 回退不应出现".to_string(),
        })
        .expect("update indexed conversation part");
    let fallback = service
        .search_conversation_records(ConversationSearchParams {
            record_kind: Some("session".to_string()),
            adapter_id: None,
            source_id: None,
            project_path: None,
            query: "Rust fallback".to_string(),
            content_types: vec![crate::backend::dto::ConversationSearchCardType::answer()],
            card_kinds: Vec::new(),
            semantic_roles: Vec::new(),
            include_questions: None,
            include_cards: None,
            since: None,
            until: None,
            timeline: false,
            limit: Some(20),
            offset: Some(0),
            search_options: None,
        })
        .expect("fall back after indexed content changes");
    assert_eq!(fallback.backend, "legacy_scan");
    assert_eq!(
        service
            .get_conversation_search_index_status()
            .expect("load stale status")
            .health,
        "stale"
    );
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn conversation_question_detail_projects_canonical_nodes_through_app_service() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-content-node-app-service-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create content node app service root");
    let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");
    let session_id = upsert_conversation_export_fixture(&service, &root, Vec::new(), None, false);
    let initial = service
        .list_conversation_questions(ConversationQuestionListParams {
            session_id,
            query: None,
            limit: Some(10),
            offset: Some(0),
        })
        .expect("load fixture question through AppService");
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0].parts.len(), 1);
    assert_eq!(initial[0].projected_content_nodes.len(), 1);
    let first_part_id = initial[0].parts[0].id.clone();
    let first_node = &initial[0].projected_content_nodes[0];
    assert_eq!(first_node.question_id, initial[0].question.id);
    assert_eq!(first_node.turn_id, initial[0].turns[0].id);
    assert_eq!(first_node.part_id, initial[0].parts[0].id);
    assert_eq!(first_node.node_order, 0);
    assert_eq!(first_node.locator.part_id, initial[0].parts[0].id);

    let question_id = initial[0].question.id.clone();
    let turn_id = initial[0].turns[0].id.clone();
    let pool = service.db.pool().clone();
    let tenant_id = service.tenant_id().to_string();
    service
        .db
        .block_on(async move {
            sqlx::query(
                r#"
                INSERT INTO conversation_parts (
                    tenant_id, id, turn_id, part_index, role, kind, text, language,
                    command, cwd, status, exit_code, metadata_json, translated_text,
                    content_card_json, source_execution_id, command_label
                )
                VALUES (?1, 'app-service-second-part', ?2, 1, 'assistant', 'text',
                    'Second projected Part', NULL, NULL, NULL, NULL, NULL, ?3, NULL,
                    NULL, 'execution-second', 'second')
                "#,
            )
            .bind(&tenant_id)
            .bind(&turn_id)
            .bind(r#"{"content_card":{"type":"answer","format":"markdown"}}"#)
            .execute(&pool)
            .await
            .map_err(AppError::external)?;
            sqlx::query(
                r#"
                INSERT INTO conversation_parts (
                    tenant_id, id, turn_id, part_index, role, kind, text, language,
                    command, cwd, status, exit_code, metadata_json, translated_text,
                    content_card_json, source_execution_id, command_label
                )
                VALUES (?1, 'app-service-empty-part', ?2, 2, 'assistant', 'text',
                    NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL)
                "#,
            )
            .bind(&tenant_id)
            .bind(&turn_id)
            .execute(&pool)
            .await
            .map_err(AppError::external)?;
            AppResult::Ok(())
        })
        .expect("seed multi-part and empty projection fixture");

    let expanded = service
        .get_conversation_question(ConversationQuestionGetParams { question_id })
        .expect("reload canonical content nodes through AppService");
    assert_eq!(expanded.parts.len(), 3);
    assert_eq!(expanded.projected_content_nodes.len(), 2);
    assert_eq!(
        expanded
            .projected_content_nodes
            .iter()
            .map(|node| node.part_id.as_str())
            .collect::<Vec<_>>(),
        vec![first_part_id.as_str(), "app-service-second-part"]
    );
    assert!(expanded
        .projected_content_nodes
        .iter()
        .all(|node| node.locator.question_id == expanded.question.id));
    assert!(!expanded
        .projected_content_nodes
        .iter()
        .any(|node| node.part_id == "app-service-empty-part"));
    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn conversation_question_detail_keeps_one_raw_codex_shell_part_node() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-codex-shell-projection-app-service-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create Codex shell projection root");
    let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");
    let session_id = upsert_conversation_export_fixture(&service, &root, Vec::new(), None, false);
    let initial = service
        .list_conversation_questions(ConversationQuestionListParams {
            session_id,
            query: None,
            limit: Some(10),
            offset: Some(0),
        })
        .expect("load Codex shell projection question");
    let question_id = initial[0].question.id.clone();
    let turn_id = initial[0].turns[0].id.clone();
    let pool = service.db.pool().clone();
    let tenant_id = service.tenant_id().to_string();
    let raw_command = "printf '%s\\n' '--- inspect ---'; rg 'quoted && value' ./src | sed 's/;/|/' && git status --short > /tmp/status.txt";
    let projection_metadata = serde_json::json!({
        "shell_execution_projection": {
            "schema_version": 1,
            "nodes": [
                {"command": "rg 'quoted && value' ./src | sed 's/;/|/'", "command_label": "inspect"},
                {"command": "git status --short > /tmp/status.txt", "command_label": "status"}
            ]
        }
    })
    .to_string();
    service
        .db
        .block_on(async move {
            sqlx::query(
                r#"
                INSERT INTO conversation_parts (
                    tenant_id, id, turn_id, part_index, role, kind, text, language,
                    command, cwd, status, exit_code, command_label, metadata_json,
                    content_card_json, source_execution_id
                )
                VALUES (?1, 'conversation-part-codex-shell-command', ?2, 1, 'tool', 'command', NULL, NULL,
                    ?3, '/tmp/project', 'failed', 1, 'exec', ?4,
                    '{"schema_version":1,"kind":"codex.command","renderer":"command"}',
                    'codex-shell-execution')
                "#,
            )
            .bind(&tenant_id)
            .bind(&turn_id)
            .bind(raw_command)
            .bind(projection_metadata)
            .execute(&pool)
            .await
            .map_err(AppError::external)?;
            sqlx::query(
                r#"
                INSERT INTO conversation_parts (
                    tenant_id, id, turn_id, part_index, role, kind, text, language,
                    command, cwd, status, exit_code, command_label, metadata_json,
                    content_card_json, source_execution_id
                )
                VALUES (?1, 'conversation-part-codex-shell-result', ?2, 2, 'tool', 'tool', 'Error: failed', NULL,
                    NULL, '/tmp/project', 'failed', 1, NULL, NULL,
                    '{"schema_version":1,"kind":"codex.result","renderer":"terminal_output"}',
                    'codex-shell-execution')
                "#,
            )
            .bind(&tenant_id)
            .bind(&turn_id)
            .execute(&pool)
            .await
            .map_err(AppError::external)?;
            AppResult::Ok(())
        })
        .expect("insert raw Codex shell execution fixture");

    let detail = service
        .get_conversation_question(ConversationQuestionGetParams { question_id })
        .expect("reload Codex shell projection through AppService");
    assert_eq!(detail.parts.len(), 3);
    assert_eq!(
        detail
            .parts
            .iter()
            .filter(|part| part.id == "conversation-part-codex-shell-command")
            .count(),
        1
    );
    let command_nodes = detail
        .projected_content_nodes
        .iter()
        .filter(|node| node.part_id == "conversation-part-codex-shell-command")
        .collect::<Vec<_>>();
    assert_eq!(command_nodes.len(), 1);
    assert_eq!(
        command_nodes
            .iter()
            .map(|node| node.content.as_str())
            .collect::<Vec<_>>(),
        vec![raw_command]
    );
    assert_eq!(
        command_nodes
            .iter()
            .map(|node| node.command_label.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("exec")]
    );
    assert!(command_nodes
        .iter()
        .all(|node| node.source_execution_id.as_deref() == Some("codex-shell-execution")));
    assert_eq!(
        detail
            .projected_content_nodes
            .iter()
            .filter(|node| node.part_id == "conversation-part-codex-shell-result")
            .count(),
        1
    );
    assert_eq!(
        detail
            .projected_content_nodes
            .iter()
            .filter(|node| node.part_id == "conversation-part-codex-shell-command")
            .count(),
        1
    );

    execute_test_sql(
        &service,
        &format!(
            r#"
            INSERT INTO conversation_parts (
                tenant_id, id, turn_id, part_index, role, kind, text, language,
                command, cwd, status, exit_code, metadata_json, translated_text,
                content_card_json, source_execution_id, command_label
            ) VALUES (
                'default', 'conversation-part-legacy-split-a', '{0}', 3, 'tool', 'command',
                NULL, NULL, 'printf legacy split first', NULL, NULL, NULL, NULL, NULL,
                '{{"schema_version":1,"kind":"codex.command","renderer":"command"}}',
                'legacy-split-execution', NULL
            );
            INSERT INTO conversation_parts (
                tenant_id, id, turn_id, part_index, role, kind, text, language,
                command, cwd, status, exit_code, metadata_json, translated_text,
                content_card_json, source_execution_id, command_label
            ) VALUES (
                'default', 'conversation-part-legacy-split-b', '{0}', 4, 'tool', 'command',
                NULL, NULL, 'printf legacy split second', NULL, NULL, NULL, NULL, NULL,
                '{{"schema_version":1,"kind":"codex.command","renderer":"command"}}',
                'legacy-split-execution', NULL
            )
            "#,
            detail.turns[0].id
        ),
    )
    .expect("seed historical split command parts");
    let historical_split_search = service
        .search_conversation_records(ConversationSearchParams {
            record_kind: Some("session".to_string()),
            adapter_id: None,
            source_id: None,
            project_path: None,
            query: "legacy split second".to_string(),
            content_types: vec![crate::backend::dto::ConversationSearchCardType::new(
                "codex.command",
            )],
            card_kinds: Vec::new(),
            semantic_roles: Vec::new(),
            include_questions: Some(false),
            include_cards: Some(true),
            since: None,
            until: None,
            timeline: false,
            limit: Some(10),
            offset: Some(0),
            search_options: None,
        })
        .expect("search historical split command Part");
    assert_eq!(historical_split_search.total_count, 1);
    assert_eq!(
        historical_split_search.hits[0].block_id,
        "conversation-part-legacy-split-b"
    );
    assert_eq!(
        historical_split_search.hits[0].part_id.as_deref(),
        Some("conversation-part-legacy-split-b")
    );

    let command_search = service
        .search_conversation_records(ConversationSearchParams {
            record_kind: Some("session".to_string()),
            adapter_id: None,
            source_id: None,
            project_path: None,
            query: "git status --short".to_string(),
            content_types: vec![crate::backend::dto::ConversationSearchCardType::new(
                "codex.command",
            )],
            card_kinds: Vec::new(),
            semantic_roles: Vec::new(),
            include_questions: Some(false),
            include_cards: Some(true),
            since: None,
            until: None,
            timeline: false,
            limit: Some(10),
            offset: Some(0),
            search_options: None,
        })
        .expect("search raw shell command Part");
    assert_eq!(command_search.total_count, 1);
    assert_eq!(
        command_search.hits[0].block_id,
        "conversation-part-codex-shell-command"
    );
    assert_eq!(
        command_search.hits[0].part_id.as_deref(),
        Some("conversation-part-codex-shell-command")
    );
    assert!(command_search.hits[0]
        .snippet
        .contains("git status --short > /tmp/status.txt"));

    let blocks = service
        .list_conversation_blocks(ConversationBlockListParams {
            question_id: detail.question.id.clone(),
        })
        .expect("list raw shell command locators");
    let command_blocks = blocks
        .iter()
        .filter(|block| block.part_id.as_deref() == Some("conversation-part-codex-shell-command"))
        .collect::<Vec<_>>();
    assert_eq!(command_blocks.len(), 1);
    assert_eq!(
        command_blocks
            .iter()
            .map(|block| block.block_id.as_str())
            .collect::<Vec<_>>(),
        vec!["conversation-part-codex-shell-command"]
    );
    let status_block = service
        .get_conversation_block(ConversationBlockGetParams {
            block_id: "conversation-part-codex-shell-command".to_string(),
        })
        .expect("load raw shell command Part");
    assert_eq!(status_block.content, raw_command);

    let report = service
        .rebuild_conversation_search_index()
        .expect("rebuild raw shell search index");
    assert_eq!(report.document_count, 6);
    let indexed_command_search = service
        .search_conversation_records(ConversationSearchParams {
            record_kind: Some("session".to_string()),
            adapter_id: None,
            source_id: None,
            project_path: None,
            query: "git status --short".to_string(),
            content_types: vec![crate::backend::dto::ConversationSearchCardType::new(
                "codex.command",
            )],
            card_kinds: Vec::new(),
            semantic_roles: Vec::new(),
            include_questions: Some(false),
            include_cards: Some(true),
            since: None,
            until: None,
            timeline: false,
            limit: Some(10),
            offset: Some(0),
            search_options: None,
        })
        .expect("search raw shell command Part in Tantivy");
    assert_eq!(indexed_command_search.backend, "tantivy");
    assert_eq!(indexed_command_search.total_count, 1);
    assert_eq!(
        indexed_command_search.hits[0].block_id,
        "conversation-part-codex-shell-command"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn recent_incremental_search_prefers_a_changed_old_session_over_unchanged_history() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-recent-incremental-search-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create recent incremental search root");
    let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");
    let adapter = ConversationAdapter {
        id: "recent-incremental-adapter".to_string(),
        name: "Recent incremental adapter".to_string(),
        kind: ConversationAdapterKind::External,
        version: "0.1.0".to_string(),
        enabled: true,
        manifest_path: None,
        executable_path: None,
        content_hash: None,
        trusted_hash: None,
        trust_state: ConversationAdapterTrustState::Trusted,
        protocol_version: Some(1),
        capabilities: Vec::new(),
        input_kinds: vec![ConversationSourceKind::Directory],
        card_contract_version: None,
        card_kinds: Vec::new(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    };
    let source = ConversationSource {
        id: "recent-incremental-source".to_string(),
        adapter_id: adapter.id.clone(),
        name: "Recent incremental source".to_string(),
        kind: ConversationSourceKind::Directory,
        location: root.to_string_lossy().to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    };
    let session =
        |external_id: &str, fingerprint: &str, answer: &str| NormalizedConversationSession {
            external_id: external_id.to_string(),
            title: Some(format!("{external_id} title")),
            project_path: Some(root.join("project").to_string_lossy().to_string()),
            started_at: Some("2025-01-01T00:00:00Z".to_string()),
            updated_at: Some("2025-01-01T00:00:00Z".to_string()),
            source_locator: None,
            source_fingerprint: Some(fingerprint.to_string()),
            turns: vec![NormalizedConversationTurn {
                external_id: format!("{external_id}-turn"),
                turn_index: 0,
                user_text: "How should we handle recall?".to_string(),
                title: None,
                started_at: None,
                ended_at: None,
                parts: vec![NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Text,
                    text: Some(answer.to_string()),
                    language: None,
                    command: None,
                    cwd: None,
                    status: None,
                    exit_code: None,
                    command_label: None,
                    source_execution_id: None,
                    content_card: None,
                    metadata_json: Some(
                        r#"{"content_card":{"type":"answer","format":"markdown"}}"#.to_string(),
                    ),
                }],
            }],
        };
    let old_changed = session(
        "old-changed",
        "v1",
        "Deep recall keeps topic context in its original answer.",
    );
    let unchanged = session(
        "unchanged",
        "v1",
        "Deep recall also has a historical answer that remains unchanged.",
    );
    let updated_old = session(
        "old-changed",
        "v2",
        "Deep recall now prioritizes recent incremental sync evidence.",
    );
    let pool = service.db.pool().clone();
    let tenant_id = service.tenant_id().to_string();
    service
        .db
        .block_on(async move {
            crate::backend::store::upsert_conversation_adapter_sqlx(&pool, &tenant_id, &adapter)
                .await
                .map_err(AppError::external)?;
            crate::backend::store::upsert_conversation_source_sqlx(&pool, &tenant_id, &source)
                .await
                .map_err(AppError::external)?;
            crate::backend::store::import_conversation_sessions_sqlx(
                &pool,
                &tenant_id,
                &source,
                &[old_changed, unchanged.clone()],
                false,
            )
            .await
            .map_err(AppError::external)?;
            crate::backend::store::import_conversation_sessions_sqlx(
                &pool,
                &tenant_id,
                &source,
                &[updated_old, unchanged],
                false,
            )
            .await
        })
        .expect("persist initial and incremental conversation syncs");

    let result = service
        .search_recent_incremental_conversation_records(ConversationIncrementalSearchParams {
            record_kind: Some("session".to_string()),
            adapter_id: None,
            source_id: None,
            project_path: None,
            query: "Deep recall".to_string(),
            content_types: vec![crate::backend::dto::ConversationSearchCardType::answer()],
            card_kinds: Vec::new(),
            semantic_roles: Vec::new(),
            include_questions: Some(false),
            include_cards: Some(true),
            recent_runs: Some(1),
            limit: Some(20),
            offset: Some(0),
            search_options: None,
        })
        .expect("search the latest incremental delta");

    assert_eq!(result.backend, "incremental_delta_scan");
    assert_eq!(result.total_count, 1);
    assert_eq!(result.hits[0].session.session.external_id, "old-changed");
    assert_eq!(
        result.hits[0]
            .incremental
            .as_ref()
            .map(|value| value.change_kind.as_str()),
        Some("updated")
    );
    assert_eq!(
        result
            .incremental
            .as_ref()
            .map(|value| value.changed_session_count),
        Some(1)
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn team_roster_rules_and_persistence_ts01() {
    use crate::backend::models::{CreateTeamInput, TeamMemberInput, TeamRole};
    use rusqlite::Connection;

    let root = std::env::temp_dir().join(format!("assetiweave-team-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create team test root");
    let db_path = root.join("app.db");

    // Scope 1: Create teams and verify validation & persistence
    {
        let service = AppService::open_with_db_path(db_path.clone()).expect("open service");

        // 1. Validation test: Team with two leaders must fail
        let two_leaders_res = service.create_team(CreateTeamInput {
            id: Some("team-invalid-1".to_string()),
            name: "Two Leaders Team".to_string(),
            description: None,
            members: vec![
                TeamMemberInput {
                    id: None,
                    role: TeamRole::Leader,
                    sort_order: Some(0),
                    agent_id: "claude-code".to_string(),
                    model: Some("claude-3-7-sonnet".to_string()),
                },
                TeamMemberInput {
                    id: None,
                    role: TeamRole::Leader,
                    sort_order: Some(1),
                    agent_id: "codex".to_string(),
                    model: Some("gpt-4o".to_string()),
                },
            ],
        });
        assert!(two_leaders_res.is_err(), "team with 2 leaders must fail");

        // 2. Validation test: Team with zero teammates must fail
        let zero_teammates_res = service.create_team(CreateTeamInput {
            id: Some("team-invalid-2".to_string()),
            name: "Zero Teammates Team".to_string(),
            description: None,
            members: vec![TeamMemberInput {
                id: None,
                role: TeamRole::Leader,
                sort_order: Some(0),
                agent_id: "claude-code".to_string(),
                model: Some("claude-3-7-sonnet".to_string()),
            }],
        });
        assert!(
            zero_teammates_res.is_err(),
            "team with 0 teammates must fail"
        );

        // 3. Valid team creation: 1 Leader + 2 Teammates with identical agent_id & model
        let valid_res = service.create_team(CreateTeamInput {
            id: Some("team-alpha".to_string()),
            name: "Alpha Engineering Team".to_string(),
            description: Some("Autonomous pair programming unit".to_string()),
            members: vec![
                TeamMemberInput {
                    id: Some("mem-leader".to_string()),
                    role: TeamRole::Leader,
                    sort_order: Some(0),
                    agent_id: "claude-code".to_string(),
                    model: Some("claude-3-7-sonnet".to_string()),
                },
                TeamMemberInput {
                    id: Some("mem-worker-1".to_string()),
                    role: TeamRole::Teammate,
                    sort_order: Some(1),
                    agent_id: "codex".to_string(),
                    model: Some("gpt-4o".to_string()),
                },
                TeamMemberInput {
                    id: Some("mem-worker-2".to_string()),
                    role: TeamRole::Teammate,
                    sort_order: Some(2),
                    agent_id: "codex".to_string(),
                    model: Some("gpt-4o".to_string()),
                },
            ],
        });
        assert!(
            valid_res.is_ok(),
            "valid team creation should succeed: {:?}",
            valid_res.err()
        );
        let team_detail = valid_res.unwrap();
        assert_eq!(team_detail.team.name, "Alpha Engineering Team");
        assert_eq!(team_detail.members.len(), 3);
        assert_ne!(
            team_detail.members[1].execution_context_key,
            team_detail.members[2].execution_context_key,
            "two identical agent/model members must receive distinct execution_context_key"
        );
    }

    // Scope 2: Reopen database and verify roster order, agent/model, and context keys remain stable
    {
        let service = AppService::open_with_db_path(db_path.clone()).expect("reopen service");
        let team = service
            .get_team("team-alpha")
            .expect("get team")
            .expect("team exists");

        assert_eq!(team.team.id, "team-alpha");
        assert_eq!(team.members.len(), 3);

        // Verify member order and roles
        assert_eq!(team.members[0].id, "mem-leader");
        assert_eq!(team.members[0].role, TeamRole::Leader);
        assert_eq!(team.members[0].sort_order, 0);

        assert_eq!(team.members[1].id, "mem-worker-1");
        assert_eq!(team.members[1].role, TeamRole::Teammate);
        assert_eq!(team.members[1].sort_order, 1);
        assert_eq!(team.members[1].agent_id, "codex");
        assert_eq!(team.members[1].model.as_deref(), Some("gpt-4o"));

        assert_eq!(team.members[2].id, "mem-worker-2");
        assert_eq!(team.members[2].role, TeamRole::Teammate);
        assert_eq!(team.members[2].sort_order, 2);
        assert_eq!(team.members[2].agent_id, "codex");
        assert_eq!(team.members[2].model.as_deref(), Some("gpt-4o"));

        assert_ne!(
            team.members[1].execution_context_key,
            team.members[2].execution_context_key
        );

        // Test update: reorder members and verify context key stability
        let updated = service
            .update_team(crate::backend::models::UpdateTeamInput {
                team_id: "team-alpha".to_string(),
                name: "Alpha Engineering Team Renamed".to_string(),
                description: None,
                members: vec![
                    TeamMemberInput {
                        id: Some("mem-leader".to_string()),
                        role: TeamRole::Leader,
                        sort_order: Some(0),
                        agent_id: "claude-code".to_string(),
                        model: Some("claude-3-7-sonnet".to_string()),
                    },
                    TeamMemberInput {
                        id: Some("mem-worker-2".to_string()),
                        role: TeamRole::Teammate,
                        sort_order: Some(1), // swapped order
                        agent_id: "codex".to_string(),
                        model: Some("gpt-4o".to_string()),
                    },
                    TeamMemberInput {
                        id: Some("mem-worker-1".to_string()),
                        role: TeamRole::Teammate,
                        sort_order: Some(2), // swapped order
                        agent_id: "codex".to_string(),
                        model: Some("gpt-4o".to_string()),
                    },
                ],
            })
            .expect("update team");

        assert_eq!(updated.members[1].id, "mem-worker-2");
        assert_eq!(updated.members[2].id, "mem-worker-1");
        assert_eq!(
            updated.members[1].execution_context_key,
            team.members[2].execution_context_key
        );
        assert_eq!(
            updated.members[2].execution_context_key,
            team.members[1].execution_context_key
        );
    }

    // Scope 3: Zero-write constraint on Conversation tables (C-D04)
    {
        let conn = Connection::open(&db_path).expect("open connection");
        for table in [
            "conversation_sessions",
            "conversation_turns",
            "conversation_parts",
            "conversation_questions",
            "conversation_question_turns",
            "conversation_question_turn_audits",
            "web_record_sessions",
            "web_record_turns",
            "web_record_parts",
            "web_record_questions",
            "web_record_question_turns",
            "conversation_sync_runs",
            "conversation_sync_deltas",
            "conversation_sync_deltas_v2",
            "conversation_data_audit_issues",
            "conversation_payload_policy_state",
            "conversation_question_redirects",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap_or_else(|error| panic!("check {table}: {error}"));
            if exists == 0 {
                continue;
            }
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap_or_else(|error| panic!("count {table}: {error}"));
            assert_eq!(count, 0, "Conversation table {table} must remain untouched");
        }
    }

    let _ = fs::remove_dir_all(root);
}

#[test]
fn team_run_freezes_review_confirmation_and_idempotent_terminal_mailbox() {
    use crate::backend::{
        models::{
            CreateTeamInput, TeamConfirmInput, TeamMemberInput, TeamReviewInput, TeamRole,
            TeamTaskState,
        },
        store,
    };

    let root = std::env::temp_dir().join(format!("assetiweave-team-run-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create Team run test root");
    let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");
    service
        .create_team(CreateTeamInput {
            id: Some("team-run".to_string()),
            name: "Run team".to_string(),
            description: None,
            members: vec![
                TeamMemberInput {
                    id: Some("leader".to_string()),
                    role: TeamRole::Leader,
                    sort_order: Some(99),
                    agent_id: "claude-code".to_string(),
                    model: None,
                },
                TeamMemberInput {
                    id: Some("worker".to_string()),
                    role: TeamRole::Teammate,
                    sort_order: Some(-10),
                    agent_id: "codex".to_string(),
                    model: None,
                },
            ],
        })
        .expect("create Team");

    let pool = service.db.pool().clone();
    let tenant_id = service.tenant_id().to_string();
    let shell = service.runtime.run_sync(store::create_team_run_shell_sqlx(
        &pool, &tenant_id, "team-run",
    ));
    let shell = shell.expect("create durable drafting shell");
    assert_eq!(
        shell.run.state,
        crate::backend::models::TeamRunState::Drafting
    );
    assert_eq!(shell.run.roster_snapshot[0].member_id, "leader");
    assert_eq!(shell.run.roster_snapshot[1].sort_order, 1);

    let run_id = shell.run.id.clone();
    let roster = shell.run.roster_snapshot.clone();
    let update_while_active = service.update_team(crate::backend::models::UpdateTeamInput {
        team_id: "team-run".to_string(),
        name: "Changed".to_string(),
        description: None,
        members: vec![
            TeamMemberInput {
                id: Some("leader".to_string()),
                role: TeamRole::Leader,
                sort_order: Some(0),
                agent_id: "claude-code".to_string(),
                model: None,
            },
            TeamMemberInput {
                id: Some("worker".to_string()),
                role: TeamRole::Teammate,
                sort_order: Some(1),
                agent_id: "codex".to_string(),
                model: None,
            },
        ],
    });
    assert!(
        update_while_active.is_err(),
        "active run must freeze its roster"
    );

    let reviewed = service
        .runtime
        .run_sync(store::complete_team_run_draft_sqlx(
            &pool,
            &tenant_id,
            &run_id,
            &[crate::backend::models::TeamTaskDraft {
                id: Some("task-one".to_string()),
                title: "Inspect fixture".to_string(),
                description: "Inspect the local fixture and report findings.".to_string(),
                recommended_member_id: "worker".to_string(),
                sort_order: Some(77),
            }],
        ))
        .expect("complete draft");
    assert_eq!(
        reviewed.run.state,
        crate::backend::models::TeamRunState::AwaitingReview
    );
    assert_eq!(reviewed.tasks[0].sort_order, 0);

    let reviewed = service
        .runtime
        .run_sync(store::review_team_run_sqlx(
            &pool,
            &tenant_id,
            &TeamReviewInput {
                run_id: run_id.clone(),
                revision: reviewed.run.revision,
                tasks: vec![crate::backend::models::TeamReviewTaskInput {
                    task_id: "task-one".to_string(),
                    owner_member_id: "worker".to_string(),
                    sort_order: 123,
                }],
            },
        ))
        .expect("save human review");
    assert_eq!(reviewed.tasks[0].owner_member_id.as_deref(), Some("worker"));
    assert_eq!(reviewed.tasks[0].sort_order, 0);
    assert_eq!(reviewed.run.roster_snapshot, roster);

    let executing = service
        .runtime
        .run_sync(store::confirm_team_run_sqlx(
            &pool,
            &tenant_id,
            &TeamConfirmInput {
                run_id: run_id.clone(),
                revision: reviewed.run.revision,
            },
        ))
        .expect("confirm reviewed run");
    assert_eq!(
        executing.run.state,
        crate::backend::models::TeamRunState::Executing
    );
    assert_eq!(executing.tasks[0].state, TeamTaskState::Queued);
    let outbox_count: i64 = service
        .runtime
        .run_sync(
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM domain_event_outbox WHERE event_type = 'team_run_confirmed'",
            )
            .fetch_one(&pool),
        )
        .expect("count confirmation outbox");
    assert_eq!(outbox_count, 1);

    let claimed = service
        .runtime
        .run_sync(store::claim_team_task_sqlx(&pool, &tenant_id, "task-one"))
        .expect("claim task")
        .expect("task is queued");
    assert_eq!(claimed.state, TeamTaskState::Running);
    let finished = service
        .runtime
        .run_sync(store::finish_team_task_sqlx(
            &pool,
            &tenant_id,
            "task-one",
            TeamTaskState::Succeeded,
            Some("done"),
            None,
        ))
        .expect("finish task");
    assert_eq!(finished.state, TeamTaskState::Succeeded);
    let repeated = service
        .runtime
        .run_sync(store::finish_team_task_sqlx(
            &pool,
            &tenant_id,
            "task-one",
            TeamTaskState::Succeeded,
            Some("different"),
            None,
        ))
        .expect("repeat terminal callback");
    assert_eq!(repeated.result.as_deref(), Some("done"));
    let terminal_mail_count: i64 = service.runtime.run_sync(sqlx::query_scalar(
        "SELECT COUNT(*) FROM team_mailbox_messages WHERE run_id = ?1 AND message_type = 'task_terminal'",
    ).bind(&run_id).fetch_one(&pool)).expect("count terminal mailbox");
    assert_eq!(terminal_mail_count, 1);

    let _ = fs::remove_dir_all(root);
}
