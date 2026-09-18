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
        *self.result.lock().expect("project fake result lock") = result.to_string();
    }
}

impl AgentExecutionRuntime for FakeRuntime {
    fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
        let result = self
            .result
            .lock()
            .expect("project fake result lock")
            .clone();
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

#[test]
fn project_document_path_is_app_owned_and_scope_hashed() {
    let paths = project_document_paths(
        Path::new("/tmp/assetiweave/app.db"),
        "tenant-a",
        "/workspace/project",
        1,
    );
    assert!(paths
        .document_path
        .starts_with("/tmp/assetiweave/memory/projects"));
    assert!(paths.document_path.ends_with("MEMORY.md"));
    assert!(!paths
        .document_path
        .to_string_lossy()
        .contains("workspace/project"));
}

#[test]
fn empty_project_output_is_rejected() {
    assert!(clean_project_markdown(" \n ").is_err());
    assert_eq!(clean_project_markdown(" # project ").unwrap(), "# project");
}

#[tokio::test(flavor = "multi_thread")]
async fn successful_project_version_is_last_success_and_failed_revision_keeps_it() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-project-memory-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create project memory fixture root");
    let db_path = root.join("app.db");
    let fake = FakeRuntime::new(r##"{"content_markdown":"# first project memory"}"##);
    let service = AppService::open_with_db_path_and_runtime(db_path.clone(), fake.clone())
        .await
        .expect("open project memory service");
    let now = "2026-08-31T01:00:00Z";
    sqlx::query(
            "INSERT INTO session_memories (tenant_id,id,session_id,source_id,source_revision,source_fingerprint,contract_version,prompt_version,status,project_path,summary,goal,result,decisions_json,verification_json,blockers_json,follow_up_json,topics_json,raw_output_json,generated_at,created_at,updated_at) VALUES ('default','session-memory-a','session-a','source-a',1,'fingerprint-a','session-memory.v1','session-memory-prompt.v1','active','/project','summary a','','','[]','[]','[]','[]','[]','{}',?1,?1,?1)",
        )
        .bind(now)
        .execute(service.db.pool())
        .await
        .expect("insert session memory fixture");

    let mut tx = service
        .db
        .pool()
        .begin()
        .await
        .map_err(AppError::Db)
        .expect("begin tx");
    let project_job_id = store::enqueue_project_memory_job_tx(&mut tx, "default", "/project", now)
        .await
        .expect("enqueue project memory")
        .expect("project job");
    tx.commit().await.map_err(AppError::Db).expect("commit tx");

    let first = service
        .run_project_memory_for_tenant_at(
            "default",
            &project_job_id,
            DateTime::parse_from_rfc3339(now)
                .expect("parse project clock")
                .with_timezone(&Utc),
            TaskContext::untracked(),
        )
        .await
        .expect("run first project consolidation")
        .expect("first project version");
    assert_eq!(first.version_number, 1);
    let project = store::load_project_memory_sqlx(service.db.pool(), "default", "/project")
        .await
        .expect("load project")
        .expect("project exists");
    let first_version_id = project
        .last_successful_version_id
        .clone()
        .expect("first last-success version");
    let document_path = project.document_path.clone().expect("document path");
    let first_document = std::fs::read_to_string(&document_path).expect("read first document");
    assert_eq!(first_document, "# first project memory");

    sqlx::query(
            "UPDATE session_memories SET source_fingerprint = 'fingerprint-b', source_revision = 2 WHERE tenant_id = 'default' AND id = 'session-memory-a'",
        )
        .execute(service.db.pool())
        .await
        .expect("revise session memory");

    let mut tx = service
        .db
        .pool()
        .begin()
        .await
        .map_err(AppError::Db)
        .expect("begin tx");
    store::enqueue_project_memory_job_tx(&mut tx, "default", "/project", "2026-08-31T01:01:00Z")
        .await
        .expect("enqueue revised project");
    tx.commit().await.map_err(AppError::Db).expect("commit tx");

    fake.set_result("{}");
    assert!(service
        .run_project_memory_for_tenant_at(
            "default",
            &project_job_id,
            DateTime::parse_from_rfc3339("2026-08-31T01:01:00Z")
                .expect("parse revised project clock")
                .with_timezone(&Utc),
            TaskContext::untracked(),
        )
        .await
        .is_err());
    let project_after = store::load_project_memory_sqlx(service.db.pool(), "default", "/project")
        .await
        .expect("load project after failure")
        .expect("project after failure");
    assert_eq!(
        project_after.last_successful_version_id,
        Some(first_version_id)
    );
    assert_eq!(
        std::fs::read_to_string(document_path).expect("read preserved document"),
        first_document
    );
    drop(service);
    let _ = std::fs::remove_dir_all(root);
}
