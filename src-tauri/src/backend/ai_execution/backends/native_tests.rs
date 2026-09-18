use super::*;
use crate::backend::{
    agents::types::{AgentEnvEntry, AgentId, DeclaredAgentCapabilities},
    ai_execution::{AiExecutionCancellation, AiExecutionLimits, AiExecutionPurpose},
};

#[test]
fn test_parse_native_models_tsv() {
    let sample =
        "model-alpha-1\tModel Alpha 1\nmodel-beta-2\tModel Beta 2\nmodel-gamma-3\tModel Gamma 3\n";
    let models = parse_native_models(sample);
    assert_eq!(models.len(), 3);
    assert_eq!(models[0].id, "model-alpha-1");
    assert_eq!(models[0].label, "Model Alpha 1");
    assert_eq!(models[1].id, "model-beta-2");
    assert_eq!(models[2].id, "model-gamma-3");
}

#[test]
fn test_parse_native_models_bare() {
    let sample = "model-alpha-1\nmodel-beta-2\n";
    let models = parse_native_models(sample);
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "model-alpha-1");
    assert_eq!(models[0].label, "model-alpha-1");
}

#[test]
fn native_text_execution_rejects_tool_and_permission_events() {
    let mut text = String::new();
    let mut response = String::new();
    let mut error = None;

    assert!(matches!(
        process_native_line(
            br#"{"event":"tool_call"}"#,
            &mut text,
            &mut response,
            &mut error,
        ),
        Err(AiExecutionError::ToolUseDenied)
    ));
    assert!(matches!(
        process_native_line(
            br#"{"event":"permission_request"}"#,
            &mut text,
            &mut response,
            &mut error,
        ),
        Err(AiExecutionError::PermissionDenied)
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn persistent_native_resume_uses_declared_session_argument() {
    let root = std::env::temp_dir().join(format!("assetiweave-native-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let record = root.join("argv.ndjson");
    let definition = AgentDefinition {
        id: AgentId::parse("fake-native").unwrap(),
        installation_id: Some("fixture-installation".to_string()),
        display_name: "Fake Native".to_string(),
        protocol: AgentProtocol::Native,
        command: {
            let base =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/fake-native-agent");
            #[cfg(windows)]
            let base = base.with_extension("cmd");
            base.to_string_lossy().into_owned()
        },
        args: vec!["unused-launch-arg-is-replaced-by-the-native-invocation".to_string()],
        env: vec![AgentEnvEntry::new(
            "ASSETIWEAVE_FAKE_NATIVE_RECORD_PATH",
            record.to_string_lossy(),
        )],
        declared_capabilities: DeclaredAgentCapabilities::native_text_with_resume(vec![
            "--session".to_string(),
            "{session_id}".to_string(),
        ]),
        availability_probe: None,
        model_discovery: None,
        session_cleanup: None,
        session_cleanup_not_found_markers: Vec::new(),
    };
    let backend = NativeExecutionBackend::new(root.join("workspaces"));
    let request = |execution_id: &str| AiExecutionRequest {
        execution_id: execution_id.to_string(),
        agent_id: definition.id.clone(),
        purpose: AiExecutionPurpose::TeamTask,
        session_mode: AgentSessionMode::Persistent,
        prompt: "fixture prompt".to_string(),
        model: Some("fixture-model".to_string()),
        limits: AiExecutionLimits::default(),
        cancellation: AiExecutionCancellation::default(),
        progress: None,
        tenant_id: Some("tenant-fixture".to_string()),
        execution_context_key: Some("member-context".to_string()),
        binding: None,
        replay: false,
        restore_only: false,
        team_tools: None,
        recall_tools: None,
        memory_generation_tools: None,
    };

    let first = backend
        .execute(&definition, request("native-first"))
        .await
        .expect("first native persistent execution");
    let binding = first
        .persistent_binding
        .clone()
        .expect("first native execution returns a binding");
    assert_eq!(first.text, "native fixture response");

    let mut resumed = request("native-second");
    resumed.binding = Some(binding.clone());
    let second = backend
        .execute(&definition, resumed)
        .await
        .expect("resumed native persistent execution");
    assert_eq!(second.text, "native fixture response");
    assert_eq!(
        second.persistent_binding.unwrap().provider_session_id,
        binding.provider_session_id
    );

    let mut restored = request("native-restore");
    restored.binding = Some(binding);
    restored.restore_only = true;
    let restored = backend
        .execute(&definition, restored)
        .await
        .expect("native restore probe");
    assert!(restored.text.is_empty());

    let records = fs::read_to_string(&record).unwrap();
    let records = records
        .split("--END--\n")
        .filter(|record| !record.is_empty())
        .map(|record| record.lines().map(str::to_string).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 3);
    let session = records[0][1].as_str();
    assert_eq!(records[1][1].as_str(), session);
    assert_eq!(records[2][1].as_str(), session);
    assert!(records[0].iter().any(|arg| arg == "fixture prompt"));
    assert!(!records[2].iter().any(|arg| arg == "fixture prompt"));

    let _ = fs::remove_dir_all(root);
}
