use super::*;
use crate::backend::models::{SourceKind, SourceOrigin};
#[test]
fn detector_order_is_stable_and_priority_driven() {
    let _source = Source {
        id: "source".into(),
        name: "source".into(),
        kind: SourceKind::Local,
        root_path: "/tmp".into(),
        scanner_kind: SourceScannerKind::Mixed,
        source_origin: SourceOrigin::LocalFolder,
        repo_root: None,
        scan_root: "/tmp".into(),
        origin_app_kind: None,
        origin_provider_id: None,
        include_globs: vec![],
        exclude_globs: vec![],
        default_kind: None,
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    };
    let ctx = DetectionCtx {
        relative_path: "prompt-rule.md",
        format: AssetFormat::Markdown,
    };
    assert_eq!(
        detect(&ctx).map(|(_, _, result)| result.kind),
        Some(AssetKind::Prompt)
    );
}
