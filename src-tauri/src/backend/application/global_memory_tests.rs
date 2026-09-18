use super::*;
use crate::backend::{
    agents::types::AgentProtocol,
    ai_execution::{
        executor::BackendFuture, AgentExecutionRuntime, AiExecutionRequest, AiExecutionResult,
    },
};
use std::sync::{Arc, Mutex};

struct FakeRuntime {
    result: Mutex<String>,
}

impl FakeRuntime {
    fn new(result: &str) -> Arc<Self> {
        Arc::new(Self {
            result: Mutex::new(result.to_string()),
        })
    }

    fn set_result(&self, result: &str) {
        *self.result.lock().expect("global fake result lock") = result.to_string();
    }
}

impl AgentExecutionRuntime for FakeRuntime {
    fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
        let result = self.result.lock().expect("global fake result lock").clone();
        Box::pin(async move {
            Ok(AiExecutionResult {
                text: result,
                agent_id: request.agent_id,
                protocol: AgentProtocol::Acp,
                requested_model: request.model,
                elapsed_ms: 1,
                persistent_binding: None,
                replay_text: None,
                session_cleanup: crate::backend::ai_execution::SessionCleanupStatus::Deleted,
            })
        })
    }
}

fn global_version() -> crate::backend::models::GlobalMemoryVersion {
    crate::backend::models::GlobalMemoryVersion {
        tenant_id: "tenant".into(),
        id: "global-v1".into(),
        version_number: 1,
        status: crate::backend::models::GlobalMemoryVersionStatus::Succeeded,
        input_fingerprint: "global-fingerprint".into(),
        source_watermark: 4,
        summary_markdown: Some("- Prefer small commits".into()),
        memory_markdown: Some("- Project index: alpha".into()),
        raw_output_json: None,
        error_message: None,
        created_at: "2026-08-31T00:00:00Z".into(),
        updated_at: "2026-08-31T00:00:00Z".into(),
    }
}

fn project_version() -> crate::backend::models::ProjectMemoryVersion {
    crate::backend::models::ProjectMemoryVersion {
        tenant_id: "tenant".into(),
        id: "project-v1".into(),
        project_id: "project-1".into(),
        version_number: 1,
        status: crate::backend::models::ProjectMemoryVersionStatus::Succeeded,
        input_fingerprint: "project-fingerprint".into(),
        source_watermark: 3,
        content_markdown: Some("- Use the project test harness".into()),
        raw_output_json: None,
        error_message: None,
        created_at: "2026-08-31T00:00:00Z".into(),
        updated_at: "2026-08-31T00:00:00Z".into(),
    }
}

#[test]
fn global_document_paths_are_app_owned_and_tenant_scoped() {
    let paths = global_document_paths(Path::new("/tmp/assetiweave/app.db"), "tenant-a", 1);
    assert!(paths.root.starts_with("/tmp/assetiweave/memory/global"));
    assert!(paths.summary_document_path.ends_with("memory_summary.md"));
    assert!(paths.memory_document_path.ends_with("MEMORY.md"));
    assert!(!paths.root.to_string_lossy().contains("tenant-a"));
}

#[test]
fn empty_global_output_is_rejected() {
    assert!(clean_global_markdown(" ").is_err());
    assert_eq!(clean_global_markdown(" # global ").unwrap(), "# global");
}

#[test]
fn context_budget_preserves_priority_and_stable_revision() {
    let global = global_version();
    let project = project_version();
    let global_only_budget = estimate_context_tokens(&format!(
        "## Global Memory\n{}\n\n## Project Index\n{}",
        global.summary_markdown.as_deref().unwrap(),
        global.memory_markdown.as_deref().unwrap()
    ));
    let first = compile_memory_context(
        "tenant",
        Some("/project"),
        "",
        global_only_budget,
        Some(&global),
        Some(&project),
        &[],
        &[],
        &[],
        &[],
    );
    let second = compile_memory_context(
        "tenant",
        Some("/project"),
        "",
        global_only_budget,
        Some(&global),
        Some(&project),
        &[],
        &[],
        &[],
        &[],
    );
    assert!(first.text.starts_with("## Global Memory"));
    assert_eq!(first.estimated_tokens, estimate_context_tokens(&first.text));
    assert!(first.estimated_tokens <= first.token_budget);
    assert_eq!(first.revision, second.revision);
    assert_eq!(first.references.len(), 1);
    assert_eq!(first.references[0].kind, "global_memory");
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_global_revision_keeps_last_success_and_documents() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-global-memory-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create global memory fixture root");
    let db_path = root.join("app.db");
    let fake = FakeRuntime::new(
        r###"{"summary_markdown":"# global v1","memory_markdown":"## projects\n- alpha"}"###,
    );
    let service = AppService::open_with_db_path_and_runtime(db_path.clone(), fake.clone())
        .await
        .expect("open global memory service");
    let now = "2026-08-31T01:00:00Z";
    let project_id = crate::backend::store::project_memory_id("default", "/alpha");
    sqlx::query(
            "INSERT INTO project_memories (tenant_id,id,project_path,created_at,updated_at) VALUES ('default',?1,'/alpha',?2,?2)",
        )
        .bind(&project_id)
        .bind(now)
        .execute(service.db.pool())
        .await
        .expect("insert project fixture");
    sqlx::query(
            "INSERT INTO project_memory_versions (tenant_id,id,project_id,version_number,status,input_fingerprint,source_watermark,content_markdown,created_at,updated_at) VALUES ('default','project-version-alpha-1',?1,1,'succeeded','project-fingerprint-alpha',1,'# alpha',?2,?2)",
        )
        .bind(&project_id)
        .bind(now)
        .execute(service.db.pool())
        .await
        .expect("insert project version fixture");
    sqlx::query(
            "UPDATE project_memories SET last_successful_version_id='project-version-alpha-1',last_successful_at=?1,last_successful_watermark=1,last_successful_input_fingerprint='project-fingerprint-alpha' WHERE tenant_id='default' AND id=?2",
        )
        .bind(now)
        .bind(&project_id)
        .execute(service.db.pool())
        .await
        .expect("point project at successful version");
    let global_job_id = {
        let mut tx = service
            .db
            .pool()
            .begin()
            .await
            .map_err(AppError::Db)
            .expect("begin tx");
        let job = crate::backend::store::enqueue_global_memory_job_tx(&mut tx, "default", now)
            .await
            .expect("enqueue global")
            .expect("global job");
        tx.commit().await.map_err(AppError::Db).expect("commit tx");
        job
    };
    service
        .run_global_memory_for_tenant_at(
            "default",
            &global_job_id,
            DateTime::parse_from_rfc3339(now)
                .expect("parse global clock")
                .with_timezone(&Utc),
            TaskContext::untracked(),
        )
        .await
        .expect("run global v1")
        .expect("global v1 exists");
    let first =
        crate::backend::store::load_global_memory_latest_version_sqlx(service.db.pool(), "default")
            .await
            .expect("load global v1")
            .expect("global v1");
    let first_id = first.id.clone();
    let paths = global_document_paths(&db_path, "default", 1);
    assert_eq!(
        std::fs::read_to_string(&paths.memory_document_path).unwrap(),
        "## projects\n- alpha"
    );

    let beta_id = crate::backend::store::project_memory_id("default", "/beta");
    sqlx::query(
            "INSERT INTO project_memories (tenant_id,id,project_path,created_at,updated_at) VALUES ('default',?1,'/beta',?2,?2)",
        )
        .bind(&beta_id)
        .bind("2026-08-31T01:01:00Z")
        .execute(service.db.pool())
        .await
        .expect("insert second project fixture");
    sqlx::query(
            "INSERT INTO project_memory_versions (tenant_id,id,project_id,version_number,status,input_fingerprint,source_watermark,content_markdown,created_at,updated_at) VALUES ('default','project-version-beta-1',?1,1,'succeeded','project-fingerprint-beta',2,'# beta','2026-08-31T01:01:00Z','2026-08-31T01:01:00Z')",
        )
        .bind(&beta_id)
        .execute(service.db.pool())
        .await
        .expect("insert second project version fixture");
    sqlx::query(
            "UPDATE project_memories SET last_successful_version_id='project-version-beta-1',last_successful_at='2026-08-31T01:01:00Z',last_successful_watermark=2,last_successful_input_fingerprint='project-fingerprint-beta' WHERE tenant_id='default' AND id=?1",
        )
        .bind(&beta_id)
        .execute(service.db.pool())
        .await
        .expect("point second project at successful version");
    {
        let mut tx = service
            .db
            .pool()
            .begin()
            .await
            .map_err(AppError::Db)
            .expect("begin tx");
        crate::backend::store::enqueue_global_memory_job_tx(
            &mut tx,
            "default",
            "2026-08-31T01:01:00Z",
        )
        .await
        .expect("enqueue revised global memory");
        tx.commit().await.map_err(AppError::Db).expect("commit tx");
    }
    fake.set_result("{}");
    assert!(service
        .run_global_memory_for_tenant_at(
            "default",
            &global_job_id,
            DateTime::parse_from_rfc3339("2026-08-31T01:01:00Z")
                .expect("parse revised global clock")
                .with_timezone(&Utc),
            TaskContext::untracked(),
        )
        .await
        .is_err());
    let after =
        crate::backend::store::load_global_memory_latest_version_sqlx(service.db.pool(), "default")
            .await
            .expect("load preserved global")
            .expect("preserved global");
    assert_eq!(after.id, first_id);
    assert_eq!(
        std::fs::read_to_string(paths.memory_document_path).unwrap(),
        "## projects\n- alpha"
    );
    drop(service);
    let _ = std::fs::remove_dir_all(root);
}
