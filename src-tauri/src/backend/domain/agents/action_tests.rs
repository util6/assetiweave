use super::*;

#[test]
fn resolves_known_actions() {
    let action = ActionId::new("memory.generation");
    let registration = resolve_action(&action).expect("should resolve known action");
    assert_eq!(registration.id, "memory.generation");
}

#[test]
fn rejects_unknown_action() {
    let action = ActionId::new("nonexistent.action");
    assert!(resolve_action(&action).is_err());
}
