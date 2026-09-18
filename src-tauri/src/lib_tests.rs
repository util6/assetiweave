use super::has_startup_self_check_arg;

#[test]
fn recognizes_startup_self_check_argument_without_matching_similar_values() {
    assert!(has_startup_self_check_arg([
        "assetiweave",
        "--startup-self-check"
    ]));
    assert!(!has_startup_self_check_arg([
        "assetiweave",
        "--startup-self-check=true"
    ]));
}
