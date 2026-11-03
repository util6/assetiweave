use super::*;
use crate::backend::domain::agents::{AgentId, AgentProtocol};
use crate::backend::domain::memory::NodeStatus;
use crate::backend::infrastructure::agent_execution::executor::BackendFuture;
use crate::backend::infrastructure::agent_execution::{
    AgentExecutionRuntime, AiExecutionError, AiExecutionRequest, AiExecutionResult,
    SessionCleanupStatus,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

struct MockAgentRuntime {
    call_count: AtomicUsize,
    prompts_received: Mutex<Vec<String>>,
    responses: Mutex<Vec<AppResult<String>>>,
}

impl MockAgentRuntime {
    fn new(responses: Vec<AppResult<String>>) -> Self {
        Self {
            call_count: AtomicUsize::new(0),
            prompts_received: Mutex::new(Vec::new()),
            responses: Mutex::new(responses),
        }
    }

    fn calls(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    fn received_prompts(&self) -> Vec<String> {
        self.prompts_received.lock().unwrap().clone()
    }
}

impl AgentExecutionRuntime for MockAgentRuntime {
    fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
        Box::pin(async move {
            let index = self.call_count.fetch_add(1, Ordering::SeqCst);
            self.prompts_received
                .lock()
                .unwrap()
                .push(request.prompt.clone());

            let responses = self.responses.lock().unwrap();
            if index < responses.len() {
                match &responses[index] {
                    Ok(text) => Ok(AiExecutionResult {
                        text: text.clone(),
                        agent_id: request.agent_id,
                        protocol: AgentProtocol::Acp,
                        requested_model: request.model,
                        elapsed_ms: 1,
                        persistent_binding: None,
                        replay_text: None,
                        session_cleanup: SessionCleanupStatus::Deleted,
                    }),
                    Err(err) => Err(AiExecutionError::Output {
                        message: err.to_string(),
                    }),
                }
            } else {
                Err(AiExecutionError::Output {
                    message: "No more responses configured".to_string(),
                })
            }
        })
    }
}

#[test]
fn test_unpeel_json_raw() {
    let raw = r#"{"action": "inspect", "turn_index": 2}"#;
    let val = unpeel_json(raw).expect("parse raw");
    assert_eq!(val["action"], "inspect");
    assert_eq!(val["turn_index"], 2);
}

#[test]
fn test_unpeel_json_code_fence() {
    let fenced_json = "```json\n{\n  \"action\": \"commit\",\n  \"title\": \"Add test\"\n}\n```";
    let val = unpeel_json(fenced_json).expect("parse fenced json");
    assert_eq!(val["action"], "commit");
    assert_eq!(val["title"], "Add test");

    let fenced_generic = "```\n{\"action\": \"commit\"}\n```";
    let val2 = unpeel_json(fenced_generic).expect("parse generic fence");
    assert_eq!(val2["action"], "commit");
}

#[test]
fn test_unpeel_json_surrounding_text() {
    let conversational = "Sure! Here is the JSON you requested:\n\n{\"action\": \"inspect\", \"turns\": [5], \"reason\": \"need turn 5\"}\n\nHope this helps!";
    let val = unpeel_json(conversational).expect("parse surrounded json");
    assert_eq!(val["action"], "inspect");
    assert_eq!(val["turns"][0], 5);
}

#[test]
fn test_unpeel_json_syntax_error() {
    let broken = "Not json at all";
    assert!(unpeel_json(broken).is_err());

    let malformed = "{\"action\": \"commit\", broken}";
    assert!(unpeel_json(malformed).is_err());
}

#[test]
fn test_parse_inspect_decision() {
    let text1 = r#"{"action": "inspect", "turn_index": 4, "reason": "need db details"}"#;
    let decision1 = parse_agent_decision(text1).expect("parse inspect");
    if let MemoryAgentDecision::Inspect(insp) = decision1 {
        assert_eq!(insp.target_turn(), Some(4));
        assert_eq!(insp.reason.as_deref(), Some("need db details"));
    } else {
        panic!("expected inspect decision");
    }

    let text2 = r#"{"action": "inspect", "turns": [7]}"#;
    let decision2 = parse_agent_decision(text2).expect("parse inspect array");
    if let MemoryAgentDecision::Inspect(insp) = decision2 {
        assert_eq!(insp.target_turn(), Some(7));
    } else {
        panic!("expected inspect decision");
    }
}

#[test]
fn test_parse_commit_decision_simplified() {
    let text = r#"{
      "action": "commit",
      "topic_id": "sqlite-perf",
      "parent_node_id": "098b0c4b#T1",
      "parent_status": "continued",
      "title": "Enable WAL mode",
      "detail": "Verified 5x read concurrency throughput",
      "status": "completed",
      "unexplored_branches": [
        {"title": "Explore Tantivy facet index", "detail": "benchmark pruning"}
      ]
    }"#;

    let decision = parse_agent_decision(text).expect("parse commit");
    if let MemoryAgentDecision::Commit(commit) = decision {
        assert_eq!(commit.topic_id.as_deref(), Some("sqlite-perf"));
        assert_eq!(commit.parent_node_id.as_deref(), Some("098b0c4b#T1"));
        assert_eq!(commit.parent_status, Some(NodeStatus::Continued));

        let payload = commit.into_payload("assetiweave", "session-123", "098b0c4b", 2);
        assert_eq!(payload.project_slug, "assetiweave");
        assert_eq!(payload.topic_id, "sqlite-perf");
        assert_eq!(payload.parent_node_id.as_deref(), Some("098b0c4b#T1"));
        assert_eq!(payload.parent_new_status, Some(NodeStatus::Continued));
        assert_eq!(payload.new_node.id, "098b0c4b#T2");
        assert_eq!(payload.new_node.status, NodeStatus::Completed);
        assert!(payload.new_node.summary.contains("Enable WAL mode"));
        assert_eq!(payload.unexplored_siblings.len(), 1);
        assert_eq!(payload.unexplored_siblings[0].id, "098b0c4b#T2.1");
        assert_eq!(
            payload.unexplored_siblings[0].status,
            NodeStatus::Unexplored
        );
    } else {
        panic!("expected commit decision");
    }
}

#[test]
fn test_parse_commit_decision_defensive_parent_sanitization() {
    // 1. parent_node_id as empty string ""
    let text_empty = r#"{
      "action": "commit",
      "parent_node_id": "",
      "title": "Root node with empty parent"
    }"#;
    let decision = parse_agent_decision(text_empty).expect("parse empty parent");
    if let MemoryAgentDecision::Commit(commit) = decision {
        let payload = commit.into_payload("assetiweave", "session-1", "098b0c4b", 1);
        assert_eq!(payload.parent_node_id, None);
        assert_eq!(payload.new_node.parent_id, None);
    } else {
        panic!("expected commit decision");
    }

    // 2. parent_node_id as "null" string
    let text_null_str = r#"{
      "action": "commit",
      "parent_node_id": "null",
      "title": "Root node with string null parent"
    }"#;
    let decision2 = parse_agent_decision(text_null_str).expect("parse null string parent");
    if let MemoryAgentDecision::Commit(commit2) = decision2 {
        let payload2 = commit2.into_payload("assetiweave", "session-1", "098b0c4b", 1);
        assert_eq!(payload2.parent_node_id, None);
        assert_eq!(payload2.new_node.parent_id, None);
    } else {
        panic!("expected commit decision");
    }

    // 3. parent_node_id as "none" string
    let text_none_str = r#"{
      "action": "commit",
      "parent_node_id": "none",
      "title": "Root node with string none parent"
    }"#;
    let decision3 = parse_agent_decision(text_none_str).expect("parse none string parent");
    if let MemoryAgentDecision::Commit(commit3) = decision3 {
        let payload3 = commit3.into_payload("assetiweave", "session-1", "098b0c4b", 1);
        assert_eq!(payload3.parent_node_id, None);
        assert_eq!(payload3.new_node.parent_id, None);
    } else {
        panic!("expected commit decision");
    }

    // 4. parent_node_id as JSON null
    let text_json_null = r#"{
      "action": "commit",
      "parent_node_id": null,
      "title": "Root node with JSON null parent"
    }"#;
    let decision4 = parse_agent_decision(text_json_null).expect("parse json null parent");
    if let MemoryAgentDecision::Commit(commit4) = decision4 {
        let payload4 = commit4.into_payload("assetiweave", "session-1", "098b0c4b", 1);
        assert_eq!(payload4.parent_node_id, None);
        assert_eq!(payload4.new_node.parent_id, None);
    } else {
        panic!("expected commit decision");
    }
}

#[tokio::test]
async fn test_inspect_and_commit_two_step_cycle() {
    let step1_resp =
        r#"{"action": "inspect", "turn_index": 2, "reason": "need db migration details"}"#;
    let step2_resp = r#"{
      "action": "commit",
      "topic_id": "migrations",
      "title": "Unified 14-digit migration schema",
      "detail": "Confirmed all 76 migrations renamed to 14 digits",
      "status": "completed"
    }"#;

    let runtime =
        MockAgentRuntime::new(vec![Ok(step1_resp.to_string()), Ok(step2_resp.to_string())]);
    let agent_id = AgentId::parse("test-agent").unwrap();

    let turns = vec![
        (
            1,
            "What migrations exist?".to_string(),
            "There are 76 migrations.".to_string(),
        ),
        (
            2,
            "Can we rename them to 14 digits?".to_string(),
            "Yes, renaming is safe.".to_string(),
        ),
    ];

    let qa_lookup = |turn: u32| -> Option<(String, String)> {
        if turn == 2 {
            Some((
                "Can we rename them to 14 digits?".to_string(),
                "Yes, renaming is safe and self-heals in ledger.".to_string(),
            ))
        } else {
            None
        }
    };

    let payload = execute_inspect_commit_loop(
        &runtime,
        &agent_id,
        None,
        "session-full-id",
        "sess1234",
        "my-project",
        2,
        &turns,
        "Existing topic: migrations",
        qa_lookup,
    )
    .await
    .expect("two step cycle succeeds");

    assert_eq!(runtime.calls(), 2);
    let prompts = runtime.received_prompts();
    assert_eq!(prompts.len(), 2);
    // Step 2 prompt should contain the injected QA text
    assert!(prompts[1].contains("Can we rename them to 14 digits?"));
    assert!(prompts[1].contains("self-heals in ledger"));
    assert!(prompts[1].contains("Step 2 Final Requirement"));

    assert_eq!(payload.project_slug, "my-project");
    assert_eq!(payload.topic_id, "migrations");
    assert_eq!(payload.new_node.id, "sess1234#T2");
    assert!(payload
        .new_node
        .summary
        .contains("Unified 14-digit migration schema"));
}

#[tokio::test]
async fn test_inspect_strict_fuse_at_max_one_round() {
    let step1_resp = r#"{"action": "inspect", "turn_index": 1}"#;
    // Step 2 attempts a second inspect: should trip fuse!
    let step2_resp = r#"{"action": "inspect", "turn_index": 2}"#;

    let runtime =
        MockAgentRuntime::new(vec![Ok(step1_resp.to_string()), Ok(step2_resp.to_string())]);
    let agent_id = AgentId::parse("test-agent").unwrap();

    let turns = vec![(1, "Q1".to_string(), "A1".to_string())];
    let qa_lookup = |_t| Some(("Q1 full".to_string(), "A1 full".to_string()));

    let payload = execute_inspect_commit_loop(
        &runtime,
        &agent_id,
        None,
        "session-full-id",
        "fuse1234",
        "my-project",
        1,
        &turns,
        "",
        qa_lookup,
    )
    .await
    .expect("fuse trips and produces safe fallback");

    // Exactly 2 calls made — strict fuse prevented 3rd call!
    assert_eq!(runtime.calls(), 2);
    assert_eq!(payload.new_node.id, "fuse1234#T1");
    assert!(payload.new_node.summary.contains("fuse tripped"));
}

#[tokio::test]
async fn test_single_retry_on_syntax_error() {
    let broken_first = "```json\n{\"action\": \"commit\", \"title\": \"Broken unclosed string";
    let valid_retry = "```json\n{\"action\": \"commit\", \"title\": \"Fixed after retry\", \"status\": \"completed\"}\n```";

    let runtime = MockAgentRuntime::new(vec![
        Ok(broken_first.to_string()),
        Ok(valid_retry.to_string()),
    ]);
    let agent_id = AgentId::parse("test-agent").unwrap();

    let turns = vec![(1, "Q".to_string(), "A".to_string())];
    let qa_lookup = |_t| None;

    let payload = execute_inspect_commit_loop(
        &runtime,
        &agent_id,
        None,
        "session-full-id",
        "retry123",
        "my-project",
        1,
        &turns,
        "",
        qa_lookup,
    )
    .await
    .expect("succeeds on 1 retry");

    assert_eq!(runtime.calls(), 2);
    let prompts = runtime.received_prompts();
    assert!(prompts[1].contains("Syntax Correction Notice"));
    assert!(prompts[1].contains("Broken unclosed string"));
    assert!(payload.new_node.summary.contains("Fixed after retry"));
}

#[tokio::test]
async fn test_retry_failure_aborts() {
    let broken_first = "Not JSON 1";
    let broken_second = "Not JSON 2";

    let runtime = MockAgentRuntime::new(vec![
        Ok(broken_first.to_string()),
        Ok(broken_second.to_string()),
    ]);
    let agent_id = AgentId::parse("test-agent").unwrap();

    let turns = vec![(1, "Q".to_string(), "A".to_string())];
    let qa_lookup = |_t| None;

    let result = execute_inspect_commit_loop(
        &runtime,
        &agent_id,
        None,
        "session-full-id",
        "fail123",
        "my-project",
        1,
        &turns,
        "",
        qa_lookup,
    )
    .await;

    assert!(result.is_err());
    assert_eq!(runtime.calls(), 2); // Tried once, retried once, then aborted.
}
