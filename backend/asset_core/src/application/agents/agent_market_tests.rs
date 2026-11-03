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
        protocol: AgentMarketProtocol::Acp,
        distribution_id: "system-antigravity".to_string(),
        distribution_type: DistributionType::Binary,
        ownership: Ownership::System,
        install_dir: None,
        resolved_program: fake_bin,
        args: vec![],
        definition_json: serde_json::json!({
            "id": "antigravity",
            "version": "1.0.0",
            "protocol": "acp"
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
                .find(|candidate| candidate.recommended)?
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
        assert_eq!(alt.resolved_version, Some(preview.target_version.clone()));
    }
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn test_zero_disk_cache_after_market_operations() {
    let root = std::env::temp_dir().join(format!("assetiweave-zero-cache-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create test root");
    let db_path = root.join("app.db");
    let service = AppService::open_with_db_path(db_path)
        .await
        .expect("open application service");

    let items = service
        .list_agent_market(AgentMarketListRequest {
            query: None,
            protocol: None,
            installed_only: false,
        })
        .await
        .expect("list market");
    assert!(!items.is_empty());

    let inspect = service
        .inspect_agent_market_item("antigravity".to_string())
        .await
        .expect("inspect item");
    assert_eq!(inspect.id, "antigravity");

    let refresh = service
        .refresh_agent_market_catalog()
        .await
        .expect("refresh market");
    assert_eq!(refresh.status, "updated");

    // Also verify preview leaves zero disk cache
    let preview = service
        .preview_agent_installation(AgentInstallPreviewRequest {
            agent_id: "antigravity".to_string(),
            catalog_version: None,
            agent_version: None,
            distribution_id: None,
            action: "install".to_string(),
        })
        .await
        .expect("preview install");
    assert_eq!(preview.action, "install");

    if let Some(home) = dirs::home_dir() {
        let legacy_cache = home.join(".assetiweave").join("cache").join("agent-market");
        assert!(
            !legacy_cache.exists(),
            "legacy cache dir must not exist at {:?}",
            legacy_cache
        );
    }
    let local_cache = root.join("cache").join("agent-market");
    assert!(
        !local_cache.exists(),
        "local cache dir must not exist at {:?}",
        local_cache
    );
    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn test_offline_lazy_seeding_on_empty_db() {
    let root = std::env::temp_dir().join(format!("assetiweave-offline-seeding-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create test root");
    let db_path = root.join("app.db");
    let service = AppService::open_with_db_path(db_path)
        .await
        .expect("open application service");

    let repo = ExtensionCatalogReleaseRepository::new(service.db.pool().clone());
    let count_before = repo
        .count_by_capability("agent")
        .await
        .expect("count before");
    assert_eq!(
        count_before, 0,
        "empty db should have 0 agent catalog releases"
    );

    let items = service
        .list_agent_market(AgentMarketListRequest {
            query: None,
            protocol: None,
            installed_only: false,
        })
        .await
        .expect("list agent market offline");

    assert_eq!(items.len(), 8, "bundled catalog has 8 curated agents");
    let count_after = repo
        .count_by_capability("agent")
        .await
        .expect("count after");
    assert_eq!(
        count_after, 8,
        "app_extension_catalog_releases must have 8 seeded agent releases"
    );

    let releases = repo
        .list_by_capability("agent")
        .await
        .expect("list releases");
    assert_eq!(releases.len(), 8);
    for release in releases {
        assert_eq!(release.capability_kind, "agent");
        assert_eq!(release.catalog_source, "assetiweave-curated");
        assert!(!release.display_name.is_empty());
        assert!(!release.artifact_url.is_empty());
    }

    let _ = fs::remove_dir_all(root);
}

#[test]
fn test_purge_legacy_agent_market_cache_disk_files() {
    use crate::backend::infrastructure::agent_market::purge_legacy_agent_market_cache_disk_files;

    if let Some(home) = dirs::home_dir() {
        let target = home.join(".assetiweave").join("cache").join("agent-market");
        fs::create_dir_all(&target).expect("create test legacy cache dir");
        fs::write(target.join("catalog-v1.json"), br#"{"test":true}"#)
            .expect("write dummy catalog");
        fs::write(target.join("upstream-versions.json"), br#"{}"#).expect("write dummy upstream");
        let sub = target.join("nested");
        fs::create_dir_all(&sub).expect("nested dir");
        fs::write(sub.join("nested.json"), br#"123"#).expect("nested file");

        assert!(target.exists(), "target dir must exist before purge");

        purge_legacy_agent_market_cache_disk_files().expect("purge must succeed");

        assert!(
            !target.exists(),
            "target dir must be completely deleted after purge"
        );

        // Test idempotence
        purge_legacy_agent_market_cache_disk_files().expect("subsequent purge should not error");
        assert!(!target.exists());
    }
}

#[tokio::test]
async fn test_invalid_or_missing_upstream_binary_target_fails_closed() {
    let root = std::env::temp_dir().join(format!("assetiweave-upstream-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create test root");
    let db_path = root.join("app.db");
    let service = AppService::open_with_db_path(db_path)
        .await
        .expect("open application service");

    service
        .ensure_agent_catalog_releases_seeded()
        .await
        .expect("seed");

    let items = service
        .list_agent_market(AgentMarketListRequest {
            query: Some("antigravity".to_string()),
            protocol: None,
            installed_only: false,
        })
        .await
        .expect("list market");
    let item = items
        .into_iter()
        .find(|i| i.id == "antigravity")
        .expect("antigravity item");
    let distribution = item
        .distributions
        .into_iter()
        .find(|d| d.recommended)
        .expect("distribution");

    let repo = AgentInstallationRepository::new(service.db.pool().clone());
    let now = chrono::Utc::now().to_rfc3339();
    let fake_bin = root.join("fake_agy");
    fs::write(&fake_bin, b"#!/bin/sh\necho 1.0.0\n").expect("write fake bin");

    let installation = AgentInstallation {
        agent_id: "antigravity".to_string(),
        installation_id: "inst-test-upstream-fail".to_string(),
        display_name: "Antigravity".to_string(),
        catalog_item_version: item.catalog_version.clone(),
        agent_version: "1.0.0".to_string(),
        protocol: item.protocol,
        distribution_id: distribution.distribution_id.clone(),
        distribution_type: distribution.distribution_type,
        ownership: distribution.ownership,
        install_dir: Some(root.clone()),
        resolved_program: fake_bin,
        args: vec![],
        definition_json: serde_json::json!({
            "id": "antigravity",
            "version": "1.0.0",
            "protocol": "acp"
        }),
        integrity_json: None,
        source_registry: "curated".to_string(),
        catalog_version: item.catalog_version.clone(),
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
        .expect("upsert initial active installation");

    let release_repo = ExtensionCatalogReleaseRepository::new(service.db.pool().clone());
    let (mut release, mut meta) = release_repo
        .get_latest_agent_meta("antigravity")
        .await
        .unwrap()
        .unwrap();
    let mut binary_targets = std::collections::HashMap::new();
    binary_targets.insert(
        "nonexistent-os-arch".to_string(),
        crate::backend::domain::agents::UpstreamBinaryTarget {
            archive: "https://invalid.example.com/other.zip".to_string(),
            cmd: Some("./other".to_string()),
            args: None,
            sha256: None,
        },
    );
    meta.upstream_info = Some(crate::backend::domain::agents::UpstreamVersionInfo {
        version: "2.0.0".to_string(),
        source: crate::backend::domain::agents::UpstreamProbeSource::AcpRegistry,
        checked_at: now.clone(),
        binary_targets,
    });
    release.release_meta_json = serde_json::to_string(&meta).unwrap();
    release_repo
        .batch_upsert(&[release])
        .await
        .expect("update release with invalid upstream");

    let preview = service
        .preview_agent_installation(AgentInstallPreviewRequest {
            agent_id: "antigravity".to_string(),
            catalog_version: None,
            agent_version: None,
            distribution_id: Some(distribution.distribution_id.clone()),
            action: "update".to_string(),
        })
        .await
        .expect("preview update should produce preview token");

    assert_eq!(preview.target_version, "2.0.0");

    let install_result = service
        .install_agent(AgentInstallStartRequest {
            agent_id: "antigravity".to_string(),
            catalog_version: preview.catalog_version.clone(),
            agent_version: preview.target_version.clone(),
            distribution_id: preview.selected_distribution.distribution_id.clone(),
            action: "update".to_string(),
            preview_token: preview.preview_token,
        })
        .await;

    assert!(
        install_result.is_err(),
        "install update with missing upstream binary target must fail"
    );
    let err_msg = format!("{:?}", install_result.unwrap_err());
    assert!(
        err_msg.contains("upstream_target_unavailable"),
        "error must be upstream_target_unavailable, got: {err_msg}"
    );

    let _ = fs::remove_dir_all(root);
}
