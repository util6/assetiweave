use std::collections::BTreeSet;

const BASELINE_RAW: &str = include_str!("baseline_commands.txt");

fn load_baseline_commands() -> BTreeSet<&'static str> {
    BASELINE_RAW
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

#[test]
fn baseline_contains_exactly_191_unique_commands() {
    let baseline = load_baseline_commands();
    assert_eq!(
        baseline.len(),
        191,
        "Baseline commands count should be exactly 191"
    );
}

#[test]
fn commands_mod_handler_matches_baseline_commands() {
    let commands_source = include_str!("mod.rs");
    let handler_start = commands_source
        .find("::tauri::generate_handler![")
        .expect("should find generate_handler in commands.rs");
    let after_start = &commands_source[handler_start..];
    let handler_end = after_start
        .find(']')
        .expect("should find closing bracket of generate_handler");
    let handler_body = &after_start["::tauri::generate_handler![".len()..handler_end];

    let registered_commands: BTreeSet<&str> = handler_body
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    let baseline = load_baseline_commands();

    let missing_in_registered: Vec<_> =
        baseline.difference(&registered_commands).copied().collect();
    let unexpected_in_registered: Vec<_> =
        registered_commands.difference(&baseline).copied().collect();

    assert!(
        missing_in_registered.is_empty(),
        "Missing commands in commands/mod.rs generate_handler: {:?}",
        missing_in_registered
    );
    assert!(
        unexpected_in_registered.is_empty(),
        "Unexpected extra commands in commands/mod.rs generate_handler: {:?}",
        unexpected_in_registered
    );
    assert_eq!(
        registered_commands.len(),
        191,
        "Total registered commands in commands.rs should be exactly 191"
    );
}
