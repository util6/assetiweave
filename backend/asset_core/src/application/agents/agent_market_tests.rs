use super::*;
use crate::backend::domain::agents::{
    AgentMarketProtocol, DistributionType, InstallationStatus, Ownership, ProtocolStatus,
    RuntimeStatus,
};
use crate::backend::store::system::AgentInstallationRepository;
use std::fs;
use uuid::Uuid;

#[tokio::test]
async fn incompatible_installation_denies_update_and_requires_reinstall() {
    let root = std::env::temp_dir().join(format!("assetiweave-market-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create test root");
    let db_path = root.join("app.db");
    let service = AppService::open_with_db_path(db_path)
        .await
        .expect("open application service");

    let repo = AgentInstallationRepository::new(service.db.pool().clone());
    let now = chrono::Utc::now().to_rfc3339();

    let fake_bin = root.join("fake_agy");
    fs::write(&fake_bin, b"#!/bin/sh\necho 1.0.0\n").expect("write fake bin");

    let installation = AgentInstallation {
        agent_id: "antigravity".to_string(),
        installation_id: "inst-test-incompatible".to_string(),
        display_name: "Antigravity".to_string(),
        catalog_item_version: "2026.03.1".to_string(),
        agent_version: "1.0.0".to_string(),
        protocol: AgentMarketProtocol::Native,
        distribution_id: "system-antigravity".to_string(),
        distribution_type: DistributionType::System,
        ownership: Ownership::System,
        install_dir: None,
        resolved_program: fake_bin,
        args: vec![],
        definition_json: serde_json::json!({
            "id": "antigravity",
            "version": "1.0.0",
            "protocol": "native"
        }),
        integrity_json: None,
        source_registry: "curated".to_string(),
        catalog_version: "2026.03.1".to_string(),
        enabled: true,
        installation_status: InstallationStatus::Incompatible,
        runtime_status: RuntimeStatus::Ready,
        runtime_error_code: Some("catalog_distribution_incompatible".to_string()),
        runtime_error_message: Some("catalog distribution incompatible".to_string()),
        runtime_checked_at: Some(now.clone()),
        protocol_status: ProtocolStatus::Failed,
        protocol_error_code: Some("catalog_distribution_incompatible".to_string()),
        protocol_error_message: Some("catalog distribution incompatible".to_string()),
        protocol_checked_at: Some(now.clone()),
        model_status: None,
        model_error_code: None,
        model_checked_at: None,
        installed_at: now.clone(),
        updated_at: now.clone(),
    };
    repo.upsert_active(&installation)
        .await
        .expect("insert installation");

    // 1. list_agent_market 中的 update_available 必须为 false
    let items = service
        .list_agent_market(AgentMarketListRequest {
            query: Some("antigravity".to_string()),
            protocol: None,
            installed_only: false,
        })
        .await
        .expect("list agent market");
    let item = items
        .iter()
        .find(|i| i.id == "antigravity")
        .expect("antigravity in market");
    assert!(
        !item.update_available,
        "incompatible installation must have update_available == false"
    );
    assert!(item.installed.is_some());

    // 2. preview action == "update" 必须报错 agent_reinstall_required
    let update_err = service
        .preview_agent_installation(AgentInstallPreviewRequest {
            agent_id: "antigravity".to_string(),
            catalog_version: None,
            agent_version: None,
            distribution_id: None,
            action: "update".to_string(),
        })
        .await
        .expect_err("update preview must fail");
    let err_desc = format!("{update_err:?}");
    assert!(
        err_desc.contains("agent_reinstall_required"),
        "expected agent_reinstall_required, got: {err_desc}"
    );

    // 3. preview action == "reinstall" 必须成功
    let reinstall_preview = service
        .preview_agent_installation(AgentInstallPreviewRequest {
            agent_id: "antigravity".to_string(),
            catalog_version: None,
            agent_version: None,
            distribution_id: None,
            action: "reinstall".to_string(),
        })
        .await
        .expect("reinstall preview should succeed");
    assert_eq!(reinstall_preview.action, "reinstall");
    assert_eq!(reinstall_preview.agent_id, "antigravity");
    assert!(reinstall_preview.current_installation.is_some());

    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn test_preview_update_aligns_candidate_resolved_version_with_target_version() {
    let root = std::env::temp_dir().join(format!("assetiweave-market-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create test root");
    let db_path = root.join("app.db");
    let service = AppService::open_with_db_path(db_path)
        .await
        .expect("open application service");

    let (market_item, distribution) = service
        .list_agent_market(AgentMarketListRequest {
            query: None,
            protocol: None,
            installed_only: false,
        })
        .await
        .expect("list agent market")
        .into_iter()
        .find_map(|item| {
            let distribution = item
                .distributions
                .iter()
                .find(|candidate| {
                    candidate.recommended && candidate.distribution_type != DistributionType::System
                })?
                .clone();
            Some((item, distribution))
        })
        .expect("catalog has a recommended managed distribution on this platform");
    let repo = AgentInstallationRepository::new(service.db.pool().clone());
    let now = chrono::Utc::now().to_rfc3339();

    let fake_bin = root.join(format!("fake_{}", market_item.id));
    fs::write(&fake_bin, b"#!/bin/sh\necho 1.0.0\n").expect("write fake bin");

    let installation = AgentInstallation {
        agent_id: market_item.id.clone(),
        installation_id: format!("inst-{}", market_item.id),
        display_name: market_item.display_name.clone(),
        catalog_item_version: market_item.catalog_version.clone(),
        agent_version: market_item.version.clone(),
        protocol: market_item.protocol,
        distribution_id: distribution.distribution_id.clone(),
        distribution_type: distribution.distribution_type,
        ownership: distribution.ownership,
        install_dir: Some(root.clone()),
        resolved_program: fake_bin,
        args: vec![],
        definition_json: serde_json::json!({
            "id": market_item.id.clone(),
            "version": market_item.version.clone(),
            "protocol": market_item.protocol.as_str()
        }),
        integrity_json: None,
        source_registry: "curated".to_string(),
        catalog_version: market_item.catalog_version,
        enabled: true,
        installation_status: InstallationStatus::Ready,
        runtime_status: RuntimeStatus::Ready,
        runtime_error_code: None,
        runtime_error_message: None,
        runtime_checked_at: Some(now.clone()),
        protocol_status: ProtocolStatus::Ready,
        protocol_error_code: None,
        protocol_error_message: None,
        protocol_checked_at: Some(now.clone()),
        model_status: None,
        model_error_code: None,
        model_checked_at: None,
        installed_at: now.clone(),
        updated_at: now.clone(),
    };
    repo.upsert_active(&installation)
        .await
        .expect("insert installation");

    let preview = service
        .preview_agent_installation(AgentInstallPreviewRequest {
            agent_id: market_item.id,
            catalog_version: None,
            agent_version: None,
            distribution_id: None,
            action: "update".to_string(),
        })
        .await
        .expect("update preview should succeed");

    assert_eq!(preview.action, "update");
    assert_eq!(
        preview.selected_distribution.resolved_version,
        Some(preview.target_version.clone())
    );
    for alt in &preview.alternatives {
        if alt.distribution_type != DistributionType::System {
            assert_eq!(alt.resolved_version, Some(preview.target_version.clone()));
        }
    }
    let _ = fs::remove_dir_all(root);
}
