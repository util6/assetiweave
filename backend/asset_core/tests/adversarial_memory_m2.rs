use asset_core::application::memory::project_path_to_slug;
use asset_core::application::memory::reasoning_loop::{execute_inspect_commit_loop, unpeel_json};
use asset_core::application::AppService;
use asset_core::domain::agents::{AgentId, AgentProtocol};
use asset_core::domain::memory::NodeStatus;
use asset_core::domain::{
    ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState,
    ConversationPartKind, ConversationPartRole, ConversationSource, ConversationSourceKind,
    NormalizedConversationPart, NormalizedConversationTurn,
};
use asset_core::infrastructure::agent_execution::executor::BackendFuture;
use asset_core::infrastructure::agent_execution::{
    AgentExecutionRuntime, AiExecutionError, AiExecutionRequest, AiExecutionResult,
    SessionCleanupStatus,
};
use asset_core::infrastructure::memory::storage::MemoryStorage;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use uuid::Uuid;

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct TestHarness {
    tmp_dir: PathBuf,
    mem_dir: PathBuf,
    db_path: PathBuf,
}

impl TestHarness {
    fn new(prefix: &str) -> Self {
        let tmp = std::env::temp_dir().join(format!("aiw-m2-adv-{prefix}-{}", Uuid::new_v4()));
        let db_dir = tmp.join("db");
        let mem_dir = tmp.join("memories");
        fs::create_dir_all(&db_dir).unwrap();
        fs::create_dir_all(&mem_dir).unwrap();
        let db_path = db_dir.join("app.db");
        Self {
            tmp_dir: tmp,
            mem_dir,
            db_path,
        }
    }

    async fn create_service(&self) -> AppService {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("ASSETIWEAVE_HOME", &self.tmp_dir);
        std::env::set_var("ASSETIWEAVE_DB_PATH", &self.db_path);
        std::env::set_var("ASSETIWEAVE_MEMORY_ROOT", &self.mem_dir);
        AppService::open_for_engine().await.unwrap()
    }

    fn storage(&self, tenant: &str) -> MemoryStorage {
        MemoryStorage::new_with_root(self.mem_dir.clone(), tenant)
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.tmp_dir).ok();
    }
}

struct MockAgentRuntime {
    call_count: AtomicUsize,
    prompts_received: Mutex<Vec<String>>,
    responses: Mutex<Vec<Result<String, String>>>,
}

impl MockAgentRuntime {
    fn new(responses: Vec<Result<String, String>>) -> Self {
        Self {
            call_count: AtomicUsize::new(0),
            prompts_received: Mutex::new(Vec::new()),
            responses: Mutex::new(responses),
        }
    }

    fn calls(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }

    fn prompts(&self) -> Vec<String> {
        self.prompts_received.lock().unwrap().clone()
    }
}

impl AgentExecutionRuntime for MockAgentRuntime {
    fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
        let index = self.call_count.fetch_add(1, Ordering::SeqCst);
        self.prompts_received
            .lock()
            .unwrap()
            .push(request.prompt.clone());

        let res = {
            let list = self.responses.lock().unwrap();
            if index < list.len() {
                list[index].clone()
            } else {
                Ok(r#"{"action": "commit", "title": "default fallback"}"#.to_string())
            }
        };

        Box::pin(async move {
            match res {
                Ok(text) => Ok(AiExecutionResult {
                    text,
                    agent_id: request.agent_id,
                    protocol: AgentProtocol::Acp,
                    requested_model: request.model,
                    elapsed_ms: 1,
                    persistent_binding: None,
                    replay_text: None,
                    session_cleanup: SessionCleanupStatus::Deleted,
                }),
                Err(msg) => Err(AiExecutionError::Output { message: msg }),
            }
        })
    }
}

// =========================================================================
// 1. Adversarial Scenario 1: unpeel_json Stress Matrix
// =========================================================================

#[test]
fn test_adv_unpeel_json_empty_and_whitespace_fences() {
    // 1. Completely empty body inside fence
    let empty_fence = "```json\n```";
    assert!(
        unpeel_json(empty_fence).is_err(),
        "Empty fence body must fail gracefully without panic"
    );

    // 2. Whitespace-only body inside fence
    let whitespace_fence = "```json\n    \n\t\n```";
    assert!(
        unpeel_json(whitespace_fence).is_err(),
        "Whitespace fence body must fail gracefully without panic"
    );

    // 3. Generic fence with no lang and empty body
    let empty_generic = "```\n```";
    assert!(
        unpeel_json(empty_generic).is_err(),
        "Empty generic fence must fail gracefully without panic"
    );

    // 4. Empty string and pure whitespace
    assert!(unpeel_json("").is_err());
    assert!(unpeel_json("   \n\t  ").is_err());
}

#[test]
fn test_adv_unpeel_json_multiple_fences_without_conflicting_braces() {
    // Multiple code blocks where explanation block does not have braces
    let multi_fence = r#"
Here is an example bash block:
```bash
echo "starting analysis"
cat config.yaml
```

And here is the final required JSON:
```json
{
  "action": "commit",
  "topic_id": "multi-fence-topic",
  "title": "Successfully parsed second fence",
  "status": "completed"
}
```

Hope this helps!
"#;

    let res = unpeel_json(multi_fence);
    assert!(
        res.is_ok(),
        "Must extract JSON when earlier fence has no conflicting braces: {:?}",
        res
    );
    let val = res.unwrap();
    assert_eq!(val["action"], "commit");
    assert_eq!(val["topic_id"], "multi-fence-topic");
    assert_eq!(val["title"], "Successfully parsed second fence");
}

#[tokio::test]
async fn test_adv_unpeel_json_conflicting_multiple_fences_triggers_retry_and_heals() {
    // When earlier fence contains code with curly braces like `fn main() { ... }`,
    // unpeel_json fails gracefully with Err (no panic).
    // The runtime MUST intercept this Err and trigger a 1-time retry that succeeds.
    let multi_fence_with_braces = r#"
Here is a code snippet with braces:
```rust
fn main() { println!("code with braces"); }
```

And the JSON:
```json
{"action": "commit", "title": "ambiguous braces"}
```
"#;
    assert!(
        unpeel_json(multi_fence_with_braces).is_err(),
        "unpeel_json returns Err when outer braces span multiple blocks"
    );

    // Now test that execute_inspect_commit_loop triggers 1-time retry on this Err and completes!
    let valid_retry_json =
        r#"{"action": "commit", "title": "Fixed on retry", "status": "completed"}"#;
    let runtime = MockAgentRuntime::new(vec![
        Ok(multi_fence_with_braces.to_string()),
        Ok(valid_retry_json.to_string()),
    ]);
    let agent_id = AgentId::parse("test-agent").unwrap();
    let turns = vec![(1, "Q".to_string(), "A".to_string())];
    let qa_lookup = |_t| None;

    let payload = execute_inspect_commit_loop(
        &runtime,
        &agent_id,
        None,
        "session-conflicting-fences",
        "conf123",
        "my-project",
        1,
        &turns,
        "",
        qa_lookup,
    )
    .await
    .expect("1-time retry recovers from conflicting multiple fences without panic");

    assert_eq!(runtime.calls(), 2);
    assert!(payload.new_node.summary.contains("Fixed on retry"));
    let prompts = runtime.prompts();
    assert!(prompts[1].contains("Syntax Correction Notice"));
}

#[test]
fn test_adv_unpeel_json_no_newline_after_fence() {
    // Some models output ```json{"action": "commit"}``` on a single line without \n
    let inline_fence = r#"```json{"action": "commit", "title": "inline fence"}```"#;
    let res = unpeel_json(inline_fence);
    assert!(
        res.is_ok(),
        "Inline fence without newline must parse: {:?}",
        res
    );
    let val = res.unwrap();
    assert_eq!(val["action"], "commit");
    assert_eq!(val["title"], "inline fence");
}

#[test]
fn test_adv_unpeel_json_nested_braces_and_code_snippets_in_json() {
    // Deeply nested JSON with curly braces inside strings and objects
    let nested_json = r#"{
      "action": "commit",
      "topic_id": "rust-syntax",
      "title": "Match expression with braces: match x { Some(v) => { v + 1 }, None => 0 }",
      "detail": "Nested object: {\"inner\": {\"level2\": [1, 2, 3]}}",
      "status": "completed",
      "unexplored_branches": [
        {"title": "Explore closure syntax |x| { x * 2 }", "detail": "benchmark {a: 1}"}
      ]
    }"#;

    let res = unpeel_json(nested_json);
    assert!(res.is_ok(), "Nested braces must parse: {:?}", res);
    let val = res.unwrap();
    assert_eq!(val["action"], "commit");
    assert!(val["title"].as_str().unwrap().contains("match x {"));
    assert!(val["detail"].as_str().unwrap().contains("Nested object"));
}

#[test]
fn test_adv_unpeel_json_broken_syntax_returns_error_not_panic() {
    // 1. Missing quotes on keys
    let unquoted = "{action: \"commit\", title: \"missing quotes\"}";
    assert!(unpeel_json(unquoted).is_err());

    // 2. Trailing comma
    let trailing_comma = "{\"action\": \"commit\", \"title\": \"trailing\",}";
    assert!(unpeel_json(trailing_comma).is_err());

    // 3. Unclosed quotes
    let unclosed = "{\"action\": \"commit\", \"title\": \"unclosed string";
    assert!(unpeel_json(unclosed).is_err());

    // 4. Raw non-JSON conversational text
    let conversational = "I don't think there is anything to commit here.";
    assert!(unpeel_json(conversational).is_err());
}

#[test]
fn test_adv_unpeel_json_unicode_emojis_and_chinese() {
    let unicode_json = r#"```json
    {
      "action": "commit",
      "topic_id": "智能资产编织 🚀",
      "title": "实现 ACP 纯文本推理与 `.state/` 继承树模型",
      "detail": "包含特殊字符: <script>alert(1)</script> & 换行\n测试",
      "status": "completed"
    }
    ```"#;

    let res = unpeel_json(unicode_json);
    assert!(res.is_ok(), "Unicode JSON must parse: {:?}", res);
    let val = res.unwrap();
    assert_eq!(val["topic_id"], "智能资产编织 🚀");
    assert!(val["title"]
        .as_str()
        .unwrap()
        .contains("实现 ACP 纯文本推理"));
}

// =========================================================================
// 2. Adversarial Scenario 2: 1-Round Fuse on Inspect
// =========================================================================

#[tokio::test]
async fn test_adv_inspect_fuse_trips_on_adversarial_double_inspect() {
    // Model returns Inspect on Step 1, AND returns Inspect again on Step 2
    let step1_resp = r#"{"action": "inspect", "turn_index": 3, "reason": "Need turn 3"}"#;
    let step2_resp =
        r#"{"action": "inspect", "turn_index": 4, "reason": "Adversarial second inspect"}"#;

    let runtime =
        MockAgentRuntime::new(vec![Ok(step1_resp.to_string()), Ok(step2_resp.to_string())]);
    let agent_id = AgentId::parse("test-agent").unwrap();

    let turns = vec![
        (1, "Q1".to_string(), "A1".to_string()),
        (2, "Q2".to_string(), "A2".to_string()),
        (3, "Q3".to_string(), "A3".to_string()),
    ];

    let qa_lookup = |t: u32| {
        if t == 3 {
            Some(("Q3 full text".to_string(), "A3 full text".to_string()))
        } else {
            None
        }
    };

    let payload = execute_inspect_commit_loop(
        &runtime,
        &agent_id,
        None,
        "session-adv-fuse",
        "advfuse1",
        "adv-project",
        3,
        &turns,
        "existing summary",
        qa_lookup,
    )
    .await
    .expect("Fuse trips and safely completes without error or infinite loop");

    // EMPIRICAL CRITICAL ASSERTIONS:
    // 1. Exactly 2 calls made (no 3rd call, no infinite loop)
    assert_eq!(
        runtime.calls(),
        2,
        "Strict fuse must limit LLM calls to exactly 2!"
    );

    // 2. Fallback commit payload generated
    assert_eq!(payload.project_slug, "adv-project");
    assert_eq!(payload.new_node.id, "advfuse1#T3");
    assert_eq!(payload.new_node.status, NodeStatus::Completed);
    assert!(
        payload.new_node.summary.contains("fuse tripped"),
        "Fallback summary must indicate fuse tripped: {}",
        payload.new_node.summary
    );
}

#[tokio::test]
async fn test_adv_syntax_retry_followed_by_inspect_and_second_syntax_retry() {
    // Step 1 Call 1: Broken syntax
    let step1_fail = "```json\n{broken: json";
    // Step 1 Call 2 (retry): Valid Inspect
    let step1_ok = r#"{"action": "inspect", "turn_index": 2}"#;
    // Step 2 Call 1: Broken syntax
    let step2_fail = "{also: broken}";
    // Step 2 Call 2 (retry): Valid Commit
    let step2_ok = r#"{
      "action": "commit",
      "topic_id": "resilience",
      "title": "Recovered through 2 retries across 2 steps",
      "status": "completed"
    }"#;

    let runtime = MockAgentRuntime::new(vec![
        Ok(step1_fail.to_string()),
        Ok(step1_ok.to_string()),
        Ok(step2_fail.to_string()),
        Ok(step2_ok.to_string()),
    ]);
    let agent_id = AgentId::parse("test-agent").unwrap();

    let turns = vec![(1, "Q1".to_string(), "A1".to_string())];
    let qa_lookup = |_t| Some(("Q2 detail".to_string(), "A2 detail".to_string()));

    let payload = execute_inspect_commit_loop(
        &runtime,
        &agent_id,
        None,
        "session-double-retry",
        "dbretry1",
        "adv-project",
        2,
        &turns,
        "",
        qa_lookup,
    )
    .await
    .expect("Must succeed after retrying both Step 1 and Step 2");

    assert_eq!(
        runtime.calls(),
        4,
        "Must execute exactly 4 calls (2 per step)"
    );
    assert_eq!(payload.new_node.id, "dbretry1#T2");
    assert!(payload
        .new_node
        .summary
        .contains("Recovered through 2 retries"));

    let prompts = runtime.prompts();
    assert_eq!(prompts.len(), 4);
    assert!(prompts[1].contains("Syntax Correction Notice"));
    assert!(prompts[3].contains("Syntax Correction Notice"));
}

#[tokio::test]
async fn test_adv_syntax_retry_exhaustion_in_step2_aborts_cleanly() {
    // Step 1: Valid inspect
    let step1 = r#"{"action": "inspect", "turn_index": 1}"#;
    // Step 2: Broken syntax twice
    let step2_fail1 = "not json 1";
    let step2_fail2 = "not json 2";

    let runtime = MockAgentRuntime::new(vec![
        Ok(step1.to_string()),
        Ok(step2_fail1.to_string()),
        Ok(step2_fail2.to_string()),
    ]);
    let agent_id = AgentId::parse("test-agent").unwrap();

    let turns = vec![(1, "Q".to_string(), "A".to_string())];
    let qa_lookup = |_t| Some(("Q".to_string(), "A".to_string()));

    let res = execute_inspect_commit_loop(
        &runtime,
        &agent_id,
        None,
        "session-exhaust",
        "exh1",
        "adv-project",
        1,
        &turns,
        "",
        qa_lookup,
    )
    .await;

    assert!(res.is_err(), "Must abort when step 2 retries are exhausted");
    assert_eq!(runtime.calls(), 3, "1 call for step 1 + 2 calls for step 2");
}

// =========================================================================
// 3. Adversarial Scenario 3: Oldest-First Incremental Ordering
// =========================================================================

fn make_turn(idx: u32, q: &str, a: &str, ts: &str) -> NormalizedConversationTurn {
    NormalizedConversationTurn {
        external_id: format!("turn-{idx}"),
        turn_index: idx as i64,
        user_text: q.to_string(),
        title: None,
        started_at: Some(ts.to_string()),
        ended_at: Some(ts.to_string()),
        model: None,
        parts: vec![NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some(a.to_string()),
            language: None,
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: None,
            metadata_json: None,
        }],
        ..Default::default()
    }
}

fn make_session(
    ext_id: &str,
    timestamp: &str,
    project_path: Option<&str>,
    turns: Vec<NormalizedConversationTurn>,
) -> asset_core::domain::NormalizedConversationSession {
    asset_core::domain::NormalizedConversationSession {
        external_id: ext_id.to_string(),
        title: Some(format!("Test Session {}", ext_id)),
        project_path: project_path.map(|s| s.to_string()),
        started_at: Some(timestamp.to_string()),
        updated_at: Some(timestamp.to_string()),
        source_locator: Some(format!("fixture://{}", ext_id)),
        source_fingerprint: Some(format!("fp-{}", ext_id)),
        turns,
        user_visible: Some(true),
        ..Default::default()
    }
}

async fn insert_adapter_source(
    service: &AppService,
    adapter_id: &str,
    source_id: &str,
) -> ConversationSource {
    let pool = service.pool();
    let tenant_id = service.tenant_id();

    let adapter = ConversationAdapter {
        id: adapter_id.to_string(),
        name: "Test Adapter".to_string(),
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
        input_kinds: vec![ConversationSourceKind::Directory],
        card_contract_version: None,
        card_kinds: Vec::new(),
        projection_version: Some(1),
        created_at: "2026-10-21T10:00:00Z".to_string(),
        updated_at: "2026-10-21T10:00:00Z".to_string(),
    };
    let source = ConversationSource {
        id: source_id.to_string(),
        adapter_id: adapter_id.to_string(),
        name: "Test Source".to_string(),
        kind: ConversationSourceKind::Directory,
        location: "/code/adv-order".to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: "2026-10-21T10:00:00Z".to_string(),
        updated_at: "2026-10-21T10:00:00Z".to_string(),
    };

    asset_core::store::upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter)
        .await
        .unwrap();
    asset_core::store::upsert_conversation_source_sqlx(pool, tenant_id, &source)
        .await
        .unwrap();
    source
}

#[tokio::test]
async fn test_adv_out_of_order_sessions_processed_chronologically() {
    let harness = TestHarness::new("adv-order");
    let service = harness.create_service().await;
    let pool = service.pool();
    let tenant_id = service.tenant_id();
    let storage = harness.storage(tenant_id);
    let source = insert_adapter_source(&service, "ad-adv-order", "src-adv-order").await;

    let now = chrono::Utc::now();
    // 5 timestamps in shuffled order
    let t_minus_24h = (now - chrono::Duration::hours(24)).to_rfc3339(); // 1st (oldest)
    let t_minus_10h = (now - chrono::Duration::hours(10)).to_rfc3339(); // 2nd
    let t_minus_5h = (now - chrono::Duration::hours(5)).to_rfc3339(); // 3rd
    let t_minus_2h = (now - chrono::Duration::hours(2)).to_rfc3339(); // 4th
    let t_minus_1h = (now - chrono::Duration::hours(1)).to_rfc3339(); // 5th (newest)

    let s1 = make_session(
        "sess-24h",
        &t_minus_24h,
        Some("/code/adv-order"),
        vec![make_turn(0, "question 24h ago", "answer 24h", &t_minus_24h)],
    );
    let s2 = make_session(
        "sess-10h",
        &t_minus_10h,
        Some("/code/adv-order"),
        vec![make_turn(0, "question 10h ago", "answer 10h", &t_minus_10h)],
    );
    let s3 = make_session(
        "sess-5h",
        &t_minus_5h,
        Some("/code/adv-order"),
        vec![make_turn(0, "question 5h ago", "answer 5h", &t_minus_5h)],
    );
    let s4 = make_session(
        "sess-2h",
        &t_minus_2h,
        Some("/code/adv-order"),
        vec![make_turn(0, "question 2h ago", "answer 2h", &t_minus_2h)],
    );
    let s5 = make_session(
        "sess-1h",
        &t_minus_1h,
        Some("/code/adv-order"),
        vec![make_turn(0, "question 1h ago", "answer 1h", &t_minus_1h)],
    );

    // Insert into DB completely scrambled: s3 (5h), s5 (1h), s1 (24h), s4 (2h), s2 (10h)
    asset_core::store::import_conversation_sessions_sqlx(
        pool,
        tenant_id,
        &source,
        &[s3.clone(), s5.clone(), s1.clone(), s4.clone(), s2.clone()],
        false,
    )
    .await
    .unwrap();

    let runtime = MockAgentRuntime::new(vec![
        Ok(r#"{"action": "commit", "title": "Commit 24h"}"#.to_string()),
        Ok(r#"{"action": "commit", "title": "Commit 10h"}"#.to_string()),
        Ok(r#"{"action": "commit", "title": "Commit 5h"}"#.to_string()),
        Ok(r#"{"action": "commit", "title": "Commit 2h"}"#.to_string()),
        Ok(r#"{"action": "commit", "title": "Commit 1h"}"#.to_string()),
    ]);

    let report = service
        .run_session_branch_workflow_at(Some(&runtime), Some(&storage), now)
        .await
        .unwrap();

    assert_eq!(report.scanned_count, 5);
    assert_eq!(report.processed_count, 5);
    assert_eq!(report.skipped_count, 0);
    assert_eq!(runtime.calls(), 5);

    // Verify STRICT chronological prompt arrival
    let prompts = runtime.prompts();
    assert_eq!(prompts.len(), 5);
    assert!(
        prompts[0].contains("question 24h ago"),
        "1st must be oldest (24h ago)"
    );
    assert!(
        prompts[1].contains("question 10h ago"),
        "2nd must be 10h ago"
    );
    assert!(prompts[2].contains("question 5h ago"), "3rd must be 5h ago");
    assert!(prompts[3].contains("question 2h ago"), "4th must be 2h ago");
    assert!(
        prompts[4].contains("question 1h ago"),
        "5th must be newest (1h ago)"
    );
}

// =========================================================================
// 4. Adversarial Scenario 4: Watermark 0-LLM Skip Matrix
// =========================================================================

#[tokio::test]
async fn test_adv_watermark_skip_matrix_and_zero_llm_calls() {
    let harness = TestHarness::new("adv-wm");
    let service = harness.create_service().await;
    let pool = service.pool();
    let tenant_id = service.tenant_id();
    let storage = harness.storage(tenant_id);
    let source = insert_adapter_source(&service, "ad-adv-wm", "src-adv-wm").await;

    let now = chrono::Utc::now();
    let ts = (now - chrono::Duration::hours(2)).to_rfc3339();
    let proj_slug = project_path_to_slug("/code/adv-order");

    // Case A: Session already at watermark (max_turn = 3, watermark = 3) -> Must SKIP (0 calls)
    let s_at_wm = make_session(
        "sess-wm-equal",
        &ts,
        Some("/code/adv-order"),
        vec![
            make_turn(0, "q0", "a0", &ts),
            make_turn(1, "q1", "a1", &ts),
            make_turn(2, "q2", "a2", &ts),
        ],
    );

    // Case B: Session watermark HIGHER than turns (max_turn = 2, watermark = 5) -> Must SKIP (0 calls)
    let s_above_wm = make_session(
        "sess-wm-above",
        &ts,
        Some("/code/adv-order"),
        vec![make_turn(0, "q0", "a0", &ts), make_turn(1, "q1", "a1", &ts)],
    );

    // Case C: Session with empty turns -> Must advance watermark to 0 and SKIP (0 calls)
    let s_empty = make_session("sess-wm-empty", &ts, Some("/code/adv-order"), Vec::new());

    // Case D: Session with fresh turns (max_turn = 1, watermark None) -> Must PROCESS (1 call)
    let s_fresh = make_session(
        "sess-wm-fresh",
        &ts,
        Some("/code/adv-order"),
        vec![make_turn(0, "fresh question", "fresh answer", &ts)],
    );

    asset_core::store::import_conversation_sessions_sqlx(
        pool,
        tenant_id,
        &source,
        &[
            s_at_wm.clone(),
            s_above_wm.clone(),
            s_empty.clone(),
            s_fresh.clone(),
        ],
        false,
    )
    .await
    .unwrap();

    // Query DB session IDs
    let id_equal: String = sqlx::query_scalar(
        "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND external_id = ?2",
    )
    .bind(tenant_id)
    .bind("sess-wm-equal")
    .fetch_one(pool)
    .await
    .unwrap();

    let id_above: String = sqlx::query_scalar(
        "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND external_id = ?2",
    )
    .bind(tenant_id)
    .bind("sess-wm-above")
    .fetch_one(pool)
    .await
    .unwrap();

    // Pre-record watermarks in .state/projects/<slug>.json
    service
        .advance_memory_session_watermark_in(&proj_slug, &id_equal, 3, &storage)
        .await
        .unwrap();
    service
        .advance_memory_session_watermark_in(&proj_slug, &id_above, 5, &storage)
        .await
        .unwrap();

    // Mock agent with only ONE response configured: if more than 1 call is made, it will panic or error
    let runtime = MockAgentRuntime::new(vec![Ok(
        r#"{"action": "commit", "title": "Fresh session processed"}"#.to_string(),
    )]);

    let report = service
        .run_session_branch_workflow_at(Some(&runtime), Some(&storage), now)
        .await
        .unwrap();

    // EMPIRICAL CRITICAL ASSERTIONS:
    // Out of 4 sessions:
    // 3 skipped (equal, above, empty)
    // 1 processed (fresh)
    assert_eq!(report.scanned_count, 4);
    assert_eq!(report.skipped_count, 3, "3 sessions must be skipped");
    assert_eq!(report.processed_count, 1, "Only 1 session processed");
    assert_eq!(
        runtime.calls(),
        1,
        "STRICT ASSERTION: Exactly 1 LLM call made; the 3 other sessions had 0 LLM calls!"
    );

    let prompts = runtime.prompts();
    assert!(
        prompts[0].contains("fresh question"),
        "Only the fresh session should have reached the LLM"
    );

    // SECOND RUN on same state:
    // Fresh session is now watermarked at 1!
    // Therefore, SECOND RUN must have STRICTLY 0 LLM calls!
    let runtime2 = MockAgentRuntime::new(Vec::new());
    let report2 = service
        .run_session_branch_workflow_at(Some(&runtime2), Some(&storage), now)
        .await
        .unwrap();

    assert_eq!(report2.scanned_count, 4);
    assert_eq!(report2.skipped_count, 4);
    assert_eq!(report2.processed_count, 0);
    assert_eq!(
        runtime2.calls(),
        0,
        "SECOND RUN MUST MAKE ZERO LLM CALLS (100% short-circuit)!"
    );
}
