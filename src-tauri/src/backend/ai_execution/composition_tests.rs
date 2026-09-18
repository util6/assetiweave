use super::*;

#[test]
fn unknown_action_fails_closed() {
    assert!(matches!(
        resolve_action(&ActionId::new("unknown")),
        Err(AppError::Validation(_))
    ));
}

#[test]
fn registrations_are_unique() {
    let mut ids = ACTIONS.iter().map(|action| action.id).collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), ACTIONS.len());
}
