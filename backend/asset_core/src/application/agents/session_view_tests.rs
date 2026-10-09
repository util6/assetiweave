use super::*;
use crate::backend::infrastructure::agent_execution::session_events::{
    SessionEventDelivery, SessionItemIdentity, SessionItemKind, SessionItemSnapshot,
    SessionItemState, SessionTaskStatus, TruncationInfo,
};
use schemars::schema_for;
use serde_json::json;

#[test]
fn agent_session_ref_serde_parity() {
    let session_ref = AgentSessionRef::new("session-123");
    let json_val = serde_json::to_value(&session_ref).expect("serialize ref");
    assert_eq!(
        json_val,
        json!({
            "schemaVersion": 1,
            "value": "session-123"
        })
    );

    let round_trip: AgentSessionRef = serde_json::from_value(json_val).expect("deserialize ref");
    assert_eq!(round_trip, session_ref);
}

#[test]
fn agent_session_view_and_items_wire_parity() {
    let session_ref = AgentSessionRef::new("sess-wire-1");
    let item = SessionItemSnapshot {
        identity: SessionItemIdentity {
            session_id: "sid-1".to_string(),
            member_id: "mid-1".to_string(),
            execution_id: "eid-1".to_string(),
            turn_id: "tid-1".to_string(),
            item_id: "item-1".to_string(),
        },
        kind: SessionItemKind::Tool,
        sequence: 12,
        delivery: SessionEventDelivery::Live,
        state: SessionItemState::Completed,
        text: Some("hello world".to_string()),
        status: Some(SessionTaskStatus::Succeeded),
        code: Some("0".to_string()),
        partial: false,
        truncation: Some(TruncationInfo {
            original_bytes: 500,
            retained_bytes: 200,
            strategy: "head_tail".to_string(),
        }),
        tool_call_id: Some("call-abc".to_string()),
        tool_name: Some("read_file".to_string()),
        tool_input: Some(json!({"path": "/foo/bar"})),
        tool_output: Some(json!({"content": "data"})),
    };

    let view = AgentSessionView {
        schema_version: 1,
        session_ref: session_ref.clone(),
        execution_id: "exec-99".to_string(),
        purpose: "test_wire".to_string(),
        mode: "interactive".to_string(),
        tenant_id: Some("tenant-1".to_string()),
        agent: AgentInfoView {
            id: "agent-x".to_string(),
            display_name: Some("Agent X".to_string()),
            model: Some("claude-3-5".to_string()),
            protocol: "acp".to_string(),
        },
        context: AgentSessionContextView {
            memory_scope: Some("global".to_string()),
            memory_job_id: Some("job-1".to_string()),
            task_id: Some("task-1".to_string()),
        },
        state: "active".to_string(),
        terminal: Some(AgentSessionTerminalView {
            state: "terminal_state".to_string(),
            code: Some("ERR".to_string()),
            message: Some("failed".to_string()),
            retryable: true,
        }),
        capabilities: AgentSessionCapabilitiesView {
            read: true,
            send: true,
            stop: true,
            retry: false,
            queue: false,
            interrupt: true,
            attach: false,
            mention: false,
            slash_command: false,
            model_select: false,
            permission_response: true,
            copy: true,
            open_artifact: true,
        },
        revision: 3,
        event_count: 10,
        items: vec![item.clone().into()],
        retention: SessionRetentionView {
            max_items: 256,
            max_events: 1024,
            max_bytes: 1048576,
            truncated: false,
            evicted_item_count: 0,
            rejected_event_count: 0,
        },
        started_at: Some("2026-09-25T01:00:00Z".to_string()),
        updated_at: "2026-09-25T01:05:00Z".to_string(),
        finished_at: None,
    };

    let val = serde_json::to_value(&view).expect("serialize AgentSessionView");

    // Top-level must use camelCase
    assert_eq!(val["schemaVersion"], 1);
    assert_eq!(val["sessionRef"]["value"], "sess-wire-1");
    assert_eq!(val["executionId"], "exec-99");
    assert_eq!(val["tenantId"], "tenant-1");
    assert_eq!(val["agent"]["displayName"], "Agent X");
    assert_eq!(val["context"]["memoryScope"], "global");
    assert_eq!(val["terminal"]["retryable"], true);
    assert_eq!(val["capabilities"]["permissionResponse"], true);
    assert_eq!(val["retention"]["maxItems"], 256);
    assert_eq!(val["startedAt"], "2026-09-25T01:00:00Z");

    // items array must preserve SessionItemSnapshot's snake_case format
    let item_val = &val["items"][0];
    assert_eq!(item_val["identity"]["session_id"], "sid-1");
    assert_eq!(item_val["identity"]["item_id"], "item-1");
    assert_eq!(item_val["kind"], "tool");
    assert_eq!(item_val["sequence"], 12);
    assert_eq!(item_val["delivery"], "live");
    assert_eq!(item_val["state"], "completed");
    assert_eq!(item_val["text"], "hello world");
    assert_eq!(item_val["status"], "succeeded");
    assert_eq!(item_val["partial"], false);
    assert_eq!(item_val["truncation"]["original_bytes"], 500);
    assert_eq!(item_val["truncation"]["strategy"], "head_tail");
    assert_eq!(item_val["tool_call_id"], "call-abc");
    assert_eq!(item_val["tool_name"], "read_file");
    assert_eq!(item_val["tool_input"]["path"], "/foo/bar");
    assert_eq!(item_val["tool_output"]["content"], "data");

    // Round-trip deserialization
    let round_trip: AgentSessionView =
        serde_json::from_value(val).expect("deserialize AgentSessionView");
    assert_eq!(round_trip, view);
}

#[test]
fn agent_session_get_result_wire_parity() {
    let unavailable = AgentSessionUnavailableView::new(AgentSessionRef::new("missing-ref"));
    let res_unavail = AgentSessionGetResult::Unavailable(unavailable.clone());
    let val_unavail = serde_json::to_value(&res_unavail).expect("serialize unavailable");

    assert_eq!(val_unavail["schemaVersion"], 1);
    assert_eq!(val_unavail["sessionRef"]["value"], "missing-ref");
    assert_eq!(val_unavail["state"], "unavailable");
    assert_eq!(val_unavail["reason"], "notFoundOrExpired");

    let params: AgentSessionGetParams = serde_json::from_value(json!({
        "sessionRef": {
            "schemaVersion": 1,
            "value": "query-ref"
        }
    }))
    .expect("deserialize params");
    assert_eq!(params.session_ref.value, "query-ref");

    let event = AgentSessionUpdatedEvent {
        session_ref: AgentSessionRef::new("event-ref"),
        revision: 42,
    };
    let val_event = serde_json::to_value(&event).expect("serialize event");
    assert_eq!(val_event["sessionRef"]["value"], "event-ref");
    assert_eq!(val_event["revision"], 42);
}

#[test]
fn agent_session_jsonschema_parity() {
    let schema_view = schema_for!(AgentSessionView);
    let schema_json = serde_json::to_value(&schema_view).expect("schema to value");
    let schema_str = serde_json::to_string(&schema_json).expect("schema string");

    assert!(schema_str.contains("schemaVersion"));
    assert!(schema_str.contains("sessionRef"));
    assert!(schema_str.contains("executionId"));
    assert!(schema_str.contains("capabilities"));
    assert!(schema_str.contains("retention"));
    assert!(schema_str.contains("items"));

    let schema_result = schema_for!(AgentSessionGetResult);
    let result_str = serde_json::to_string(&schema_result).expect("result schema");
    assert!(result_str.contains("unavailable") || result_str.contains("sessionRef"));

    let schema_params = schema_for!(AgentSessionGetParams);
    let params_str = serde_json::to_string(&schema_params).expect("params schema");
    assert!(params_str.contains("sessionRef"));
}

#[test]
fn session_item_view_snapshot_wire_parity() {
    let item_snapshot = SessionItemSnapshot {
        identity: SessionItemIdentity {
            session_id: "sid-1".to_string(),
            member_id: "mid-1".to_string(),
            execution_id: "eid-1".to_string(),
            turn_id: "tid-1".to_string(),
            item_id: "item-1".to_string(),
        },
        kind: SessionItemKind::Tool,
        sequence: 12,
        delivery: SessionEventDelivery::Live,
        state: SessionItemState::Completed,
        text: Some("hello world".to_string()),
        status: Some(SessionTaskStatus::Succeeded),
        code: Some("0".to_string()),
        partial: false,
        truncation: Some(TruncationInfo {
            original_bytes: 500,
            retained_bytes: 200,
            strategy: "head_tail".to_string(),
        }),
        tool_call_id: Some("call-abc".to_string()),
        tool_name: Some("read_file".to_string()),
        tool_input: Some(json!({"path": "/foo/bar"})),
        tool_output: Some(json!({"content": "data"})),
    };

    let item_view: AgentSessionItemView = item_snapshot.clone().into();
    let snap_val = serde_json::to_value(&item_snapshot).expect("serialize snapshot");
    let view_val = serde_json::to_value(&item_view).expect("serialize view");

    assert_eq!(snap_val, view_val);

    // Test reverse conversion
    let round_trip_snap: SessionItemSnapshot = item_view.into();
    assert_eq!(round_trip_snap, item_snapshot);

    // Test Schema parity for properties
    let snap_schema = serde_json::to_value(&schema_for!(SessionItemSnapshot)).expect("snap schema");
    let view_schema =
        serde_json::to_value(&schema_for!(AgentSessionItemView)).expect("view schema");

    let snap_props: Vec<_> = snap_schema["properties"]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    let view_props: Vec<_> = view_schema["properties"]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();

    assert_eq!(snap_props, view_props);
}
