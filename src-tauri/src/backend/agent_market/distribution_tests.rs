use super::*;
use crate::backend::agent_market::catalog::bundled_catalog;

#[test]
fn recommends_platform_binary_when_no_system_distribution_exists() {
    let item = bundled_catalog()
        .unwrap()
        .items
        .into_iter()
        .find(|item| item.id == "opencode")
        .unwrap();
    let context = DistributionSelectionContext {
        os: "darwin".to_string(),
        arch: "aarch64".to_string(),
        node_available: true,
        npm_available: true,
        uv_available: false,
        system: HashMap::from([(
            "opencode".to_string(),
            SystemObservation {
                resolved_program: Some(PathBuf::from("/usr/local/bin/opencode")),
                version: Some("1.2.0".to_string()),
                error_code: None,
            },
        )]),
    };
    let candidates = DistributionSelector::select(&item, &context, None).unwrap();
    assert_eq!(candidates[0].distribution_id, "binary-darwin-aarch64");
    assert!(candidates[0].recommended);
    assert!(candidates
        .iter()
        .any(|candidate| candidate.distribution_type == DistributionType::Binary));
}

#[test]
fn rust_macos_host_name_matches_darwin_catalog_target() {
    let item = bundled_catalog()
        .unwrap()
        .items
        .into_iter()
        .find(|item| item.id == "opencode")
        .unwrap();
    let context = DistributionSelectionContext {
        os: normalized_os("macos"),
        arch: normalized_arch("aarch64"),
        node_available: false,
        npm_available: false,
        uv_available: false,
        system: HashMap::new(),
    };

    let candidate = DistributionSelector::select(&item, &context, None)
        .unwrap()
        .remove(0);

    assert!(candidate.selectable);
    assert!(candidate.recommended);
}

#[test]
fn explicit_unavailable_choice_is_not_silently_replaced() {
    let item = bundled_catalog()
        .unwrap()
        .items
        .into_iter()
        .find(|item| item.id == "qoder")
        .unwrap();
    let error = DistributionSelector::select(
        &item,
        &DistributionSelectionContext::default(),
        Some("npx-qoder"),
    )
    .expect_err("missing uv must reject explicit choice");
    assert_eq!(error, "runtime_missing");
}

#[test]
fn npx_requires_both_node_and_npm() {
    let item = bundled_catalog()
        .unwrap()
        .items
        .into_iter()
        .find(|item| item.id == "claude")
        .unwrap();
    let mut context = DistributionSelectionContext::default();
    context.node_available = true;
    context.npm_available = false;
    let candidate = DistributionSelector::select(&item, &context, None)
        .unwrap()
        .remove(0);
    assert!(!candidate.selectable);
    assert_eq!(candidate.reason_code.as_deref(), Some("runtime_missing"));
}
