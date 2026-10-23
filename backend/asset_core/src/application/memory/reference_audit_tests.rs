use super::*;
use sqlx::SqlitePool;

async fn open_service(name: &str) -> (AppService, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-memory-reference-{name}-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create memory reference test root");
    let service = AppService::open_with_db_path(root.join("app.db"))
        .await
        .expect("open application service");
    (service, root)
}

async fn seed(pool: &SqlitePool, statements: &[&str]) {
    let mut connection = pool.acquire().await.expect("acquire seed connection");
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *connection)
        .await
        .expect("disable foreign keys for fixtures");
    for statement in statements {
        sqlx::query(sqlx::AssertSqlSafe(statement.to_string()))
            .execute(&mut *connection)
            .await
            .unwrap_or_else(|error| panic!("seed statement failed: {error}\n{statement}"));
    }
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *connection)
        .await
        .expect("restore foreign keys after fixtures");
}

fn session_fixtures() -> Vec<&'static str> {
    vec![
        "INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, title, missing, created_at, imported_at) VALUES ('default', 's-1', 'src-1', 'adp-1', 'ext-1', 'Session one', 0, '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
        "INSERT INTO conversation_turns (tenant_id, id, session_id, external_id, turn_index, user_text, fingerprint, missing, imported_at) VALUES ('default', 't-1', 's-1', 'ext-t-1', 0, 'first prompt', 'fp-t-1', 0, '2026-10-23T00:00:00Z')",
        "INSERT INTO conversation_questions (tenant_id, id, session_id, title, created_at, updated_at) VALUES ('default', 'q-1', 's-1', 'first', '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
        "INSERT INTO conversation_question_turns (tenant_id, question_id, turn_id, turn_order) VALUES ('default', 'q-1', 't-1', 0)",
    ]
}

#[tokio::test(flavor = "multi_thread")]
async fn audit_reports_five_counts_and_keeps_database_untouched() {
    let (service, root) = open_service("audit").await;
    seed_orphan_fixtures(&service).await;

    let audit = service
        .audit_memory_references()
        .await
        .expect("audit memory references");

    assert_eq!(audit.counts.total_references, 4);
    assert_eq!(audit.counts.valid, 1);
    assert_eq!(audit.counts.orphaned, 3);
    assert_eq!(audit.counts.repaired, 0);
    assert_eq!(audit.counts.manual_review, 0);
    assert_eq!(audit.repairs.len(), 0);
    assert_eq!(audit.planned_repairs.len(), 3);
    assert!(audit.dry_run);

    let pending_orphans: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memory_evidence_snapshots WHERE tenant_id = 'default' AND question_id = 'q-gone'",
    )
    .fetch_one(service.db.pool())
    .await
    .expect("count orphan evidence after audit");
    assert_eq!(pending_orphans, 1);

    drop(service);
    fs::remove_dir_all(root).ok();
}

async fn seed_orphan_fixtures(service: &AppService) {
    let mut statements = session_fixtures();
    statements.extend([
        "INSERT INTO memory_evidence_snapshots (tenant_id, id, record_kind, session_id, question_id, turn_id, block_id, content_hash, excerpt, created_at, updated_at) VALUES ('default', 'e-valid', 'session', 's-1', 'q-1', 't-1', 'block-e-valid', 'hash-e-valid', 'excerpt', '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
        "INSERT INTO memory_evidence_snapshots (tenant_id, id, record_kind, session_id, question_id, turn_id, block_id, content_hash, excerpt, created_at, updated_at) VALUES ('default', 'e-orphan', 'session', 's-1', 'q-gone', 't-1', 'block-e-orphan', 'hash-e-orphan', 'excerpt', '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
        "INSERT INTO memory_evidence_snapshots (tenant_id, id, record_kind, session_id, question_id, turn_id, block_id, content_hash, excerpt, created_at, updated_at) VALUES ('default', 'e-without-question', 'session', 's-1', NULL, 't-1', 'block-e-without-question', 'hash-e-without-question', 'excerpt', '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
        "INSERT INTO session_memory_source_references (tenant_id, id, memory_id, source_id, session_id, record_kind, question_id, reference_key, source_revision, created_at) VALUES ('default', 'r-orphan', 'memory-1', 'src-1', 's-1', 'session', 'q-gone', 'ref-r-orphan', 1, '2026-10-23T00:00:00Z')",
        "INSERT INTO memory_item_source_references (tenant_id, id, item_revision_id, record_kind, source_id, session_id, question_id, reference_key, source_revision, availability, created_at) VALUES ('default', 'm-orphan', 'revision-1', 'session', 'src-1', 's-1', 'q-gone', 'ref-m-orphan', 1, 'available', '2026-10-23T00:00:00Z')",
    ]);
    seed(service.db.pool(), &statements).await;
}

async fn reference_question(service: &AppService, table: &str, id: &str) -> Option<String> {
    let sql = format!("SELECT question_id FROM {table} WHERE tenant_id = 'default' AND id = ?1");
    sqlx::query_scalar::<_, Option<String>>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_one(service.db.pool())
        .await
        .expect("read reference question id")
}

#[tokio::test(flavor = "multi_thread")]
async fn repair_requires_confirmation_and_dry_run_writes_nothing() {
    let (service, root) = open_service("repair-gate").await;
    seed_orphan_fixtures(&service).await;

    let refused = service
        .repair_memory_references(MemoryReferenceRepairParams {
            dry_run: false,
            yes: false,
        })
        .await;
    assert!(refused.is_err());

    let dry_run = service
        .repair_memory_references(MemoryReferenceRepairParams {
            dry_run: true,
            yes: true,
        })
        .await
        .expect("dry-run repair");
    assert!(dry_run.dry_run);
    assert_eq!(dry_run.counts.orphaned, 3);
    assert_eq!(dry_run.counts.repaired, 0);
    assert_eq!(dry_run.repairs.len(), 0);
    assert_eq!(
        reference_question(&service, "memory_evidence_snapshots", "e-orphan").await,
        Some("q-gone".to_string())
    );
    assert_eq!(
        reference_question(&service, "session_memory_source_references", "r-orphan").await,
        Some("q-gone".to_string())
    );
    let ledger_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memory_reference_audit_findings WHERE tenant_id = 'default'",
    )
    .fetch_one(service.db.pool())
    .await
    .expect("count findings after dry-run");
    assert_eq!(ledger_rows, 0);

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn question_from_another_session_counts_as_orphan() {
    let (service, root) = open_service("cross-session").await;
    seed(
        service.db.pool(),
        &[
            "INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, title, missing, created_at, imported_at) VALUES ('default', 's-5', 'src-1', 'adp-1', 'ext-5', 'Session five', 0, '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
            "INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, title, missing, created_at, imported_at) VALUES ('default', 's-6', 'src-1', 'adp-1', 'ext-6', 'Session six', 0, '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
            "INSERT INTO conversation_questions (tenant_id, id, session_id, title, created_at, updated_at) VALUES ('default', 'q-5', 's-5', 'five', '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
            "INSERT INTO conversation_questions (tenant_id, id, session_id, title, created_at, updated_at) VALUES ('default', 'q-6', 's-6', 'six', '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
            "INSERT INTO memory_evidence_snapshots (tenant_id, id, record_kind, session_id, question_id, turn_id, block_id, content_hash, excerpt, created_at, updated_at) VALUES ('default', 'e-cross', 'session', 's-5', 'q-6', NULL, 'block-e-cross', 'hash-e-cross', 'excerpt', '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
        ],
    )
    .await;

    let audit = service
        .audit_memory_references()
        .await
        .expect("audit cross-session reference");
    assert_eq!(audit.counts.orphaned, 1);
    assert_eq!(audit.counts.valid, 0);

    let repaired = service
        .repair_memory_references(MemoryReferenceRepairParams {
            dry_run: false,
            yes: true,
        })
        .await
        .expect("repair cross-session reference");
    assert_eq!(repaired.repairs[0].resolved_question_id, "q-5");
    assert_eq!(
        reference_question(&service, "memory_evidence_snapshots", "e-cross").await,
        Some("q-5".to_string())
    );

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn repair_remaps_orphans_and_second_run_is_a_no_op() {
    let (service, root) = open_service("repair-apply").await;
    seed_orphan_fixtures(&service).await;

    let repaired = service
        .repair_memory_references(MemoryReferenceRepairParams {
            dry_run: false,
            yes: true,
        })
        .await
        .expect("apply repair");
    assert!(!repaired.dry_run);
    assert_eq!(repaired.counts.total_references, 4);
    assert_eq!(repaired.counts.valid, 1);
    assert_eq!(repaired.counts.repaired, 3);
    assert_eq!(repaired.counts.orphaned, 0);
    assert_eq!(repaired.counts.manual_review, 0);
    assert_eq!(repaired.repairs.len(), 3);
    assert_eq!(repaired.planned_repairs.len(), 3);
    assert!(repaired.planned_invalidations.is_empty());
    assert!(repaired.manual_review.is_empty());

    let strategies = repaired
        .repairs
        .iter()
        .map(|repair| (repair.reference_id.as_str(), repair.strategy.as_str()))
        .collect::<HashMap<_, _>>();
    assert_eq!(strategies["e-orphan"], "turn_membership");
    assert_eq!(strategies["r-orphan"], "single_session_group");
    assert_eq!(strategies["m-orphan"], "single_session_group");

    for (table, id) in [
        ("memory_evidence_snapshots", "e-orphan"),
        ("session_memory_source_references", "r-orphan"),
        ("memory_item_source_references", "m-orphan"),
    ] {
        assert_eq!(
            reference_question(&service, table, id).await,
            Some("q-1".to_string()),
            "{table} was not remapped"
        );
    }

    let second = service
        .repair_memory_references(MemoryReferenceRepairParams {
            dry_run: false,
            yes: true,
        })
        .await
        .expect("repeat repair");
    assert!(second.repairs.is_empty());
    assert_eq!(second.counts, repaired.counts);
    let audit = service
        .audit_memory_references()
        .await
        .expect("audit after repair");
    assert_eq!(audit.counts, repaired.counts);

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn undeterminable_orphans_are_invalidated_instead_of_deleted() {
    let (service, root) = open_service("invalidate").await;
    seed(
        service.db.pool(),
        &[
            "INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, title, missing, created_at, imported_at) VALUES ('default', 's-2', 'src-1', 'adp-1', 'ext-2', 'Session two', 0, '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
            "INSERT INTO conversation_turns (tenant_id, id, session_id, external_id, turn_index, user_text, fingerprint, missing, imported_at) VALUES ('default', 't-2a', 's-2', 'ext-t-2a', 0, 'first', 'fp-t-2a', 0, '2026-10-23T00:00:00Z')",
            "INSERT INTO conversation_turns (tenant_id, id, session_id, external_id, turn_index, user_text, fingerprint, missing, imported_at) VALUES ('default', 't-2b', 's-2', 'ext-t-2b', 1, 'second', 'fp-t-2b', 0, '2026-10-23T00:01:00Z')",
            "INSERT INTO conversation_questions (tenant_id, id, session_id, title, created_at, updated_at) VALUES ('default', 'q-2a', 's-2', 'first', '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
            "INSERT INTO conversation_questions (tenant_id, id, session_id, title, created_at, updated_at) VALUES ('default', 'q-2b', 's-2', 'second', '2026-10-23T00:01:00Z', '2026-10-23T00:01:00Z')",
            "INSERT INTO conversation_question_turns (tenant_id, question_id, turn_id, turn_order) VALUES ('default', 'q-2a', 't-2a', 0)",
            "INSERT INTO conversation_question_turns (tenant_id, question_id, turn_id, turn_order) VALUES ('default', 'q-2b', 't-2b', 0)",
            "INSERT INTO memory_item_source_references (tenant_id, id, item_revision_id, record_kind, source_id, session_id, question_id, reference_key, source_revision, availability, created_at) VALUES ('default', 'm-ambiguous', 'revision-2', 'session', 'src-1', 's-2', 'q-vanished', 'ref-m-ambiguous', 1, 'available', '2026-10-23T00:02:00Z')",
        ],
    )
    .await;

    let audit = service
        .audit_memory_references()
        .await
        .expect("audit ambiguous orphan");
    assert_eq!(audit.counts.total_references, 1);
    assert_eq!(audit.counts.orphaned, 1);
    assert_eq!(audit.counts.manual_review, 0);
    assert_eq!(audit.planned_invalidations.len(), 1);
    assert_eq!(audit.planned_invalidations[0].reason, "ambiguous_group");

    let repaired = service
        .repair_memory_references(MemoryReferenceRepairParams {
            dry_run: false,
            yes: true,
        })
        .await
        .expect("apply invalidation");
    assert!(repaired.repairs.is_empty());
    assert_eq!(repaired.counts.orphaned, 0);
    assert_eq!(repaired.counts.manual_review, 1);
    assert_eq!(repaired.manual_review.len(), 1);
    assert_eq!(repaired.manual_review[0].reference_id, "m-ambiguous");
    assert_eq!(repaired.manual_review[0].reason, "ambiguous_group");
    assert_eq!(repaired.manual_review[0].question_id, "q-vanished");

    let (availability, reason, question_id): (String, Option<String>, String) = sqlx::query_as(
        "SELECT availability, unavailable_reason, question_id FROM memory_item_source_references WHERE tenant_id = 'default' AND id = 'm-ambiguous'",
    )
    .fetch_one(service.db.pool())
    .await
    .expect("read invalidated reference");
    assert_eq!(availability, "unavailable");
    assert_eq!(reason.as_deref(), Some("missing"));
    assert_eq!(question_id, "q-vanished");

    let second = service
        .repair_memory_references(MemoryReferenceRepairParams {
            dry_run: false,
            yes: true,
        })
        .await
        .expect("repeat invalidation");
    assert!(second.repairs.is_empty());
    assert!(second.planned_invalidations.is_empty());
    assert_eq!(second.counts, repaired.counts);

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn orphan_without_membership_remaps_to_the_nearest_group() {
    let (service, root) = open_service("nearest").await;
    seed(
        service.db.pool(),
        &[
            "INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, title, missing, created_at, imported_at) VALUES ('default', 's-3', 'src-1', 'adp-1', 'ext-3', 'Session three', 0, '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
            "INSERT INTO conversation_turns (tenant_id, id, session_id, external_id, turn_index, user_text, fingerprint, missing, imported_at) VALUES ('default', 't-3a', 's-3', 'ext-t-3a', 0, 'first', 'fp-t-3a', 0, '2026-10-23T00:00:00Z')",
            "INSERT INTO conversation_turns (tenant_id, id, session_id, external_id, turn_index, user_text, fingerprint, missing, imported_at) VALUES ('default', 't-3b', 's-3', 'ext-t-3b', 10, 'second', 'fp-t-3b', 0, '2026-10-23T00:10:00Z')",
            "INSERT INTO conversation_turns (tenant_id, id, session_id, external_id, turn_index, user_text, fingerprint, missing, imported_at) VALUES ('default', 't-3-loose', 's-3', 'ext-t-3-loose', 2, 'loose', 'fp-t-3-loose', 0, '2026-10-23T00:02:00Z')",
            "INSERT INTO conversation_turns (tenant_id, id, session_id, external_id, turn_index, user_text, fingerprint, missing, imported_at) VALUES ('default', 't-3-tie', 's-3', 'ext-t-3-tie', 5, 'tie', 'fp-t-3-tie', 0, '2026-10-23T00:05:00Z')",
            "INSERT INTO conversation_questions (tenant_id, id, session_id, title, created_at, updated_at) VALUES ('default', 'q-3a', 's-3', 'first', '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
            "INSERT INTO conversation_questions (tenant_id, id, session_id, title, created_at, updated_at) VALUES ('default', 'q-3b', 's-3', 'second', '2026-10-23T00:10:00Z', '2026-10-23T00:10:00Z')",
            "INSERT INTO conversation_question_turns (tenant_id, question_id, turn_id, turn_order) VALUES ('default', 'q-3a', 't-3a', 0)",
            "INSERT INTO conversation_question_turns (tenant_id, question_id, turn_id, turn_order) VALUES ('default', 'q-3b', 't-3b', 0)",
            "INSERT INTO memory_evidence_snapshots (tenant_id, id, record_kind, session_id, question_id, turn_id, block_id, content_hash, excerpt, created_at, updated_at) VALUES ('default', 'e-nearest', 'session', 's-3', 'q-vanished', 't-3-loose', 'block-e-nearest', 'hash-e-nearest', 'excerpt', '2026-10-23T00:02:00Z', '2026-10-23T00:02:00Z')",
            "INSERT INTO memory_evidence_snapshots (tenant_id, id, record_kind, session_id, question_id, turn_id, block_id, content_hash, excerpt, created_at, updated_at) VALUES ('default', 'e-tie', 'session', 's-3', 'q-vanished', 't-3-tie', 'block-e-tie', 'hash-e-tie', 'excerpt', '2026-10-23T00:05:00Z', '2026-10-23T00:05:00Z')",
        ],
    )
    .await;

    let audit = service
        .audit_memory_references()
        .await
        .expect("audit nearest group");
    assert_eq!(audit.planned_repairs.len(), 1);
    assert_eq!(audit.planned_repairs[0].reference_id, "e-nearest");
    assert_eq!(audit.planned_invalidations.len(), 1);
    assert_eq!(audit.planned_invalidations[0].reference_id, "e-tie");
    assert_eq!(audit.planned_invalidations[0].reason, "ambiguous_group");

    let repaired = service
        .repair_memory_references(MemoryReferenceRepairParams {
            dry_run: false,
            yes: true,
        })
        .await
        .expect("repair nearest group");
    assert_eq!(repaired.repairs.len(), 1);
    assert_eq!(repaired.repairs[0].strategy, "nearest_group");
    assert_eq!(repaired.repairs[0].resolved_question_id, "q-3a");
    assert_eq!(repaired.counts.manual_review, 1);
    assert_eq!(
        reference_question(&service, "memory_evidence_snapshots", "e-nearest").await,
        Some("q-3a".to_string())
    );
    assert_eq!(
        reference_question(&service, "memory_evidence_snapshots", "e-tie").await,
        Some("q-vanished".to_string())
    );

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn merged_question_tombstone_is_followed_before_any_guess() {
    let (service, root) = open_service("redirect").await;
    seed(
        service.db.pool(),
        &[
            "INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, title, missing, created_at, imported_at) VALUES ('default', 's-4', 'src-1', 'adp-1', 'ext-4', 'Session four', 0, '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
            "INSERT INTO conversation_questions (tenant_id, id, session_id, title, created_at, updated_at) VALUES ('default', 'q-4-survivor', 's-4', 'survivor', '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
            "INSERT INTO conversation_question_redirects (tenant_id, source_question_id, target_question_id, operation_kind, created_at, updated_at) VALUES ('default', 'q-4-merged', 'q-4-survivor', 'merge', '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
            "INSERT INTO session_memory_source_references (tenant_id, id, memory_id, source_id, session_id, record_kind, question_id, reference_key, source_revision, created_at) VALUES ('default', 'r-redirect', 'memory-4', 'src-1', 's-4', 'session', 'q-4-merged', 'ref-r-redirect', 1, '2026-10-23T00:00:00Z')",
        ],
    )
    .await;

    let repaired = service
        .repair_memory_references(MemoryReferenceRepairParams {
            dry_run: false,
            yes: true,
        })
        .await
        .expect("repair through tombstone");
    assert_eq!(repaired.repairs.len(), 1);
    assert_eq!(repaired.repairs[0].strategy, "redirect_tombstone");
    assert_eq!(repaired.repairs[0].resolved_question_id, "q-4-survivor");

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn web_record_references_are_audited_against_web_questions() {
    let (service, root) = open_service("web").await;
    seed(
        service.db.pool(),
        &[
            "INSERT INTO web_record_questions (tenant_id, id, session_id, title, created_at, updated_at) VALUES ('default', 'wq-1', 'ws-1', 'web', '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
            "INSERT INTO memory_evidence_snapshots (tenant_id, id, record_kind, session_id, question_id, turn_id, block_id, content_hash, excerpt, created_at, updated_at) VALUES ('default', 'e-web-valid', 'web', 'ws-1', 'wq-1', NULL, 'block-e-web-valid', 'hash-e-web-valid', 'excerpt', '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
            "INSERT INTO memory_evidence_snapshots (tenant_id, id, record_kind, session_id, question_id, turn_id, block_id, content_hash, excerpt, created_at, updated_at) VALUES ('default', 'e-web-orphan', 'web', 'ws-1', 'wq-gone', NULL, 'block-e-web-orphan', 'hash-e-web-orphan', 'excerpt', '2026-10-23T00:00:00Z', '2026-10-23T00:00:00Z')",
        ],
    )
    .await;

    let audit = service
        .audit_memory_references()
        .await
        .expect("audit web references");
    assert_eq!(audit.counts.total_references, 2);
    assert_eq!(audit.counts.valid, 1);
    assert_eq!(audit.counts.orphaned, 1);

    let repaired = service
        .repair_memory_references(MemoryReferenceRepairParams {
            dry_run: false,
            yes: true,
        })
        .await
        .expect("repair web references");
    assert_eq!(repaired.counts.orphaned, 0);
    assert_eq!(repaired.counts.repaired, 1);
    assert_eq!(repaired.repairs[0].strategy, "single_session_group");
    assert_eq!(repaired.repairs[0].resolved_question_id, "wq-1");

    drop(service);
    fs::remove_dir_all(root).ok();
}
