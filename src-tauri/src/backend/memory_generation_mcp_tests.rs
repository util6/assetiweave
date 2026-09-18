use super::*;

#[test]
fn tool_surface_matches_the_memory_generation_allowlist() {
    let tools = tools_result();
    let names = tools["tools"]
        .as_array()
        .expect("tool array")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        crate::backend::models::ALLOWED_MEMORY_GENERATION_TOOLS
    );
}

#[test]
fn tool_budget_rejects_calls_after_the_fixed_limit() {
    let mut budget = ToolBudget::default();
    for _ in 0..MAX_TOOL_CALLS {
        budget.begin_call().expect("call within budget");
    }
    assert!(budget.begin_call().is_err());
}

#[test]
fn tool_budget_rejects_oversized_single_response() {
    let mut budget = ToolBudget::default();
    let value = Value::String("x".repeat(MAX_SINGLE_RESPONSE_BYTES + 1));
    assert!(budget.record_response(&value).is_err());
}
