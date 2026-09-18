use super::*;
use crate::backend::agent_market::types::AgentMarketProtocol;

#[test]
fn bundled_catalog_contains_all_initial_agents_without_execution_commands() {
    let catalog = bundled_catalog().expect("bundled catalog");
    let ids = catalog
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    for id in [
        "opencode",
        "gemini",
        "antigravity",
        "claude",
        "codex",
        "pi",
        "qoder",
    ] {
        assert!(ids.contains(id), "missing {id}");
    }
    let antigravity = catalog
        .items
        .iter()
        .find(|item| item.id == "antigravity")
        .expect("Antigravity catalog item");
    assert_eq!(antigravity.protocol, AgentMarketProtocol::Acp);
    assert_eq!(antigravity.version, "1.1.1");
    assert_eq!(
        antigravity.verification.status,
        crate::backend::agent_market::types::VerificationStatus::Experimental
    );
    assert!(!antigravity.capabilities.resume);
    assert!(!antigravity.capabilities.history_replay);
    assert!(!antigravity.capabilities.live_events);
    assert_eq!(antigravity.distributions.len(), 5);
    for d in &antigravity.distributions {
        assert!(matches!(d, Distribution::Binary { .. }));
    }
    let json = serde_json::to_string(&catalog).expect("catalog json");
    assert!(!json.contains("npx -y"));
    assert!(!json.contains("latest"));
}

#[test]
fn invalid_cache_does_not_parse_as_catalog() {
    let error =
        CatalogService::from_bytes(br#"{"schema":"bad"}"#).expect_err("invalid cache must fail");
    assert!(matches!(
        error,
        CatalogError::InvalidJson(_) | CatalogError::Invalid(_)
    ));
}

#[test]
fn preview_token_changes_when_selected_distribution_changes() {
    let service = CatalogService::bundled().expect("bundled catalog");
    let item = service.item("opencode").expect("OpenCode item");
    let first = service.preview_token(item, item.distributions[0].id(), "install");
    let second = service.preview_token(
        item,
        &format!("{}-alternate", item.distributions[0].id()),
        "install",
    );
    assert_ne!(first, second);
    assert_eq!(first.len(), 24);
}

#[test]
fn preview_token_is_bound_to_exact_lifecycle_action() {
    let service = CatalogService::bundled().expect("bundled catalog");
    let item = service.item("opencode").expect("OpenCode item");
    let install = service.preview_token(item, item.distributions[0].id(), "install");
    let update = service.preview_token(item, item.distributions[0].id(), "update");
    let reinstall = service.preview_token(item, item.distributions[0].id(), "reinstall");

    assert_ne!(install, update);
    assert_ne!(update, reinstall);
    assert_ne!(install, reinstall);
}

#[test]
fn preview_token_ignores_observational_catalog_and_agent_versions() {
    let catalog = bundled_catalog().expect("bundled catalog");
    let service = CatalogService::from_catalog(catalog.clone());
    let item = service.item("opencode").expect("OpenCode item");
    let distribution_id = item.distributions[0].id().to_string();
    let token = service.preview_token(item, &distribution_id, "install");

    let mut changed = catalog;
    changed.catalog_version = "2099.01.01.1".to_string();
    changed.items[0].version = "999.0.0".to_string();
    let changed_service = CatalogService::from_catalog(changed);
    let changed_item = changed_service.item("opencode").expect("OpenCode item");

    assert_eq!(
        token,
        changed_service.preview_token(changed_item, &distribution_id, "install")
    );
}

#[test]
fn bundled_opencode_keeps_cli_model_discovery_in_its_runtime_definition() {
    let service = CatalogService::bundled().expect("bundled catalog");
    let item = service.item("opencode").expect("OpenCode item");

    assert!(item.capabilities.model_discovery);
    let model_discovery_args = match &item.distributions[0] {
        Distribution::Binary {
            model_discovery_args,
            ..
        } => model_discovery_args,
        other => panic!("unexpected OpenCode distribution: {other:?}"),
    };
    assert_eq!(
        model_discovery_args.as_deref(),
        Some(["models".to_string()].as_slice())
    );
}

#[test]
fn bundled_opencode_maps_session_delete_args_into_its_runtime_definition() {
    let service = CatalogService::bundled().expect("bundled catalog");
    let item = service.item("opencode").expect("OpenCode item");

    let session_cleanup_args = match &item.distributions[0] {
        Distribution::Binary {
            session_cleanup_args,
            ..
        } => session_cleanup_args,
        other => panic!("unexpected OpenCode distribution: {other:?}"),
    };
    assert_eq!(
        session_cleanup_args.as_deref(),
        Some(
            ["session", "delete", "{session_id}"]
                .map(str::to_string)
                .as_slice()
        )
    );
}
