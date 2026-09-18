use super::*;
use std::fs;

async fn setup() -> (AppService, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-memory-generation-tools-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).unwrap();
    let service = AppService::open_with_db_path(root.join("app.db"))
        .await
        .unwrap();
    let now = "2026-09-17T08:00:00Z";
    let source_reference = serde_json::json!({
        "tenant_id": "default",
        "id": "source-reference-row-id",
        "memory_id": "memory-fixture",
        "source_id": "source-fixture",
        "session_id": "session-fixture",
        "question_id": "question-fixture",
        "turn_id": "turn-fixture",
        "part_id": null,
        "node_id": null,
        "node_order": null,
        "reference_key": "session-memory-ref-persistent-secret",
        "source_revision": 1,
        "created_at": now
    });
    let work_order_json = serde_json::json!({
        "payload": {
            "targetWatermarkUtc": now,
            "localWatermarkDate": "2026-09-17",
            "localWatermarkTime": "16:00",
            "timezoneOffsetMinutes": 480,
            "windowHours": 48,
            "windowStartUtc": "2026-09-15T08:00:00Z",
            "windowEndUtc": now,
            "targetFingerprint": "target-fingerprint",
            "contentFingerprint": "content-fingerprint",
            "skill": {
                "assetId": "skill-fixture",
                "assetRevision": 1,
                "contentHash": "content-hash",
                "entryHash": "entry-hash"
            },
            "skillText": "Generate recent memory.",
            "evidence": {
                "targetWatermarkUtc": now,
                "windowStartUtc": "2026-09-15T08:00:00Z",
                "windowEndUtc": now,
                "windowHours": 48,
                "projectKeys": ["project-fixture"],
                "candidateSessions": [],
                "sessionEvidence": [{
                    "candidate": {
                        "sessionRef": "SES-FIXTURE",
                        "sessionId": "session-fixture",
                        "projectKey": "project-fixture",
                        "title": "Fixture session",
                        "lastActivityAt": now,
                        "sourceId": "source-fixture",
                        "sourceAgent": "codex",
                        "sourceRevision": 1
                    },
                    "memorySourceRevision": 1,
                    "summary": "Frozen summary",
                    "goal": null,
                    "result": null,
                    "decisions": [],
                    "verification": [],
                    "blockers": [],
                    "followUp": [],
                    "topics": ["memory"],
                    "sourceReferences": [source_reference],
                    "recentEvents": []
                }],
                "continuableItems": [],
                "currentL2Projects": [],
                "currentL3Items": [],
                "allowedTools": [
                    "get_session_outline",
                    "search_session_content",
                    "read_question_content",
                    "read_content_node"
                ],
                "outputSchemaVersion": 2
            }
        }
    })
    .to_string();
    store::enqueue_recent_memory_job_sqlx(
        service.db.pool(),
        service.tenant_id(),
        "job-fixture",
        now,
        48,
        "target-fingerprint",
        "content-fingerprint",
        &work_order_json,
        now,
    )
    .await
    .unwrap();
    sqlx::query(
        "UPDATE recent_memory_jobs SET status = 'running', ownership_token = ?1 WHERE tenant_id = ?2 AND id = ?3",
    )
    .bind("lease-fixture")
    .bind(service.tenant_id())
    .bind("job-fixture")
    .execute(service.db.pool())
    .await
    .unwrap();
    (service, root)
}

#[tokio::test(flavor = "multi_thread")]
async fn memory_generation_tool_is_bound_to_the_active_job_lease_and_frozen_scope() {
    let (service, root) = setup().await;

    let outline = service
        .call_memory_generation_tool(
            "job-fixture",
            "lease-fixture",
            "get_session_outline",
            &serde_json::json!({ "session_ref": "SES-FIXTURE" }),
        )
        .await
        .unwrap();
    assert_eq!(outline["candidate"]["sessionRef"], "SES-FIXTURE");
    assert_eq!(outline["facts"]["summary"], "Frozen summary");
    assert_eq!(outline["nodes"][0]["nodeRef"], "SES-FIXTURE.r1");
    assert!(!outline
        .to_string()
        .contains("session-memory-ref-persistent-secret"));

    assert!(service
        .call_memory_generation_tool(
            "job-fixture",
            "wrong-lease",
            "get_session_outline",
            &serde_json::json!({ "session_ref": "SES-FIXTURE" }),
        )
        .await
        .is_err());
    assert!(service
        .call_memory_generation_tool(
            "job-fixture",
            "lease-fixture",
            "get_session_outline",
            &serde_json::json!({ "session_ref": "SES-OUTSIDE" }),
        )
        .await
        .is_err());
    assert!(service
        .call_memory_generation_tool(
            "job-fixture",
            "lease-fixture",
            "exec",
            &serde_json::json!({}),
        )
        .await
        .is_err());

    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "multi_thread")]
async fn memory_generation_tool_stops_work_after_the_job_lease_ends() {
    let (service, root) = setup().await;
    sqlx::query(
        "UPDATE recent_memory_jobs SET status = 'succeeded', ownership_token = NULL WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(service.tenant_id())
    .bind("job-fixture")
    .execute(service.db.pool())
    .await
    .unwrap();

    let result = service
        .call_memory_generation_tool(
            "job-fixture",
            "lease-fixture",
            "get_session_outline",
            &serde_json::json!({ "session_ref": "SES-FIXTURE" }),
        )
        .await;

    assert!(result.is_err());
    let _ = fs::remove_dir_all(root);
}
