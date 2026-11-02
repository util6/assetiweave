use super::*;

#[test]
fn app_kind_uses_frontend_compatible_names() {
    assert_eq!(
        serde_json::to_string(&AppKind::OpenCode).unwrap(),
        "\"opencode\""
    );
    assert_eq!(
        serde_json::to_string(&AppKind::OpenClaw).unwrap(),
        "\"openclaw\""
    );
    assert_eq!(
        serde_json::from_str::<AppKind>("\"open_code\"").unwrap(),
        AppKind::OpenCode
    );
    assert_eq!(
        serde_json::from_str::<AppKind>("\"open_claw\"").unwrap(),
        AppKind::OpenClaw
    );
    assert_eq!(
        serde_json::to_string(&AppKind::Workbuddy).unwrap(),
        "\"workbuddy\""
    );
    assert_eq!(
        serde_json::from_str::<AppKind>("\"work_buddy\"").unwrap(),
        AppKind::Workbuddy
    );
}
