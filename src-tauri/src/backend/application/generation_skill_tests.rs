use super::*;
use crate::backend::models::{
    MemoryJobPurpose, MemoryScopeV2, MemoryWindowV2, MemoryWorkOrderV2,
    ALLOWED_MEMORY_GENERATION_TOOLS,
};

#[tokio::test(flavor = "multi_thread")]
async fn test_generation_skill_lifecycle_and_work_order() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-generation-skill-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let db_path = root.join("app.db");

    let service = AppService::open_with_db_path(db_path)
        .await
        .expect("open service");

    // 1. 默认无配置时，解析出系统内置模板绑定
    let default_binding = service
        .get_active_generation_skill_binding()
        .await
        .expect("get default skill binding");
    assert_eq!(default_binding.asset_id, MEMORY_GENERATION_SKILL_ID);
    assert_eq!(default_binding.asset_revision, 1);
    assert!(!default_binding.content_hash.is_empty());
    assert!(!default_binding.entry_hash.is_empty());

    // 2. 复制内置模板到用户 Library
    let duplicated_asset = service
        .duplicate_generation_skill_to_library()
        .await
        .expect("duplicate skill");
    assert_ne!(duplicated_asset.asset.id, MEMORY_GENERATION_SKILL_ID);
    assert_eq!(duplicated_asset.asset.kind, AssetKind::Skill);

    // 校验设置已自动保存该新 Skill ID
    let active_binding = service
        .get_active_generation_skill_binding()
        .await
        .expect("get custom skill binding");
    assert_eq!(active_binding.asset_id, duplicated_asset.asset.id);
    assert_eq!(active_binding.asset_revision, 1);
    assert!(!active_binding.content_hash.is_empty());

    // 3. 构建 Work Order V2 并验证绑定
    let work_order = MemoryWorkOrderV2::new(
        "wo-001".to_string(),
        service.tenant_id().to_string(),
        MemoryJobPurpose::RecentSnapshot,
        "2026-09-15T14:00:00Z".to_string(),
        MemoryWindowV2 {
            start_utc: "2026-09-13T14:00:00Z".to_string(),
            end_utc: "2026-09-15T14:00:00Z".to_string(),
            hours: 48,
        },
        MemoryScopeV2 {
            project_key: Some("assetiweave".to_string()),
        },
        "src-rev-hash-001".to_string(),
        active_binding.clone(),
        "2026-09-15T14:01:00Z".to_string(),
    );

    assert_eq!(work_order.contract_version, "memory.contract.v2");
    assert_eq!(work_order.budget_policy_version, "budget.v1");
    assert_eq!(work_order.projection_policy_version, "projection.v2");
    assert_eq!(work_order.skill.asset_id, duplicated_asset.asset.id);
    assert!(!work_order.input_fingerprint.is_empty());

    // 4. M35-SKILL-05: 严格工具白名单测试 (越权拦截)
    for allowed in ALLOWED_MEMORY_GENERATION_TOOLS {
        assert!(work_order.is_allowed_tool(allowed));
    }
    assert!(!work_order.is_allowed_tool("run_shell_command"));
    assert!(!work_order.is_allowed_tool("write_file"));
    assert!(!work_order.is_allowed_tool("fetch_network_url"));
    assert!(!work_order.is_allowed_tool("spawn_subagent"));
    assert!(!work_order.is_allowed_tool("sql_query"));

    // 5. 恢复默认模板测试
    service
        .reset_generation_skill_to_default()
        .await
        .expect("reset to default");
    let restored_binding = service
        .get_active_generation_skill_binding()
        .await
        .expect("get restored binding");
    assert_eq!(restored_binding.asset_id, MEMORY_GENERATION_SKILL_ID);

    // 6. 无效 Skill 测试：验证前置拒绝且不静默回退
    let invalid_check = service
        .validate_generation_skill_asset("non-existent-asset-id")
        .await;
    assert!(invalid_check.is_err());
    let err_msg = invalid_check.unwrap_err().to_string();
    assert!(err_msg.contains("MEMORY_SKILL_NOT_FOUND"));

    // 7. Schedule 校验测试 (24/48/72 与水位相异)
    let valid_settings = serde_json::json!({
        "memory": {
            "recentWindowHours": 48,
            "watermarkTime1": "02:00",
            "watermarkTime2": "14:00"
        }
    });
    assert!(service
        .validate_memory_settings(&valid_settings)
        .await
        .is_ok());

    let invalid_window = serde_json::json!({
        "memory": {
            "recentWindowHours": 36,
            "watermarkTime1": "02:00",
            "watermarkTime2": "14:00"
        }
    });
    assert!(service
        .validate_memory_settings(&invalid_window)
        .await
        .is_err());

    let identical_watermarks = serde_json::json!({
        "memory": {
            "recentWindowHours": 48,
            "watermarkTime1": "14:00",
            "watermarkTime2": "14:00"
        }
    });
    assert!(service
        .validate_memory_settings(&identical_watermarks)
        .await
        .is_err());

    drop(service);
    let _ = fs::remove_dir_all(root);
}
