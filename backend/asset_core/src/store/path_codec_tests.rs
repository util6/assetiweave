use super::*;

#[test]
fn test_normalize_empty_and_anchored() {
    assert_eq!(normalize_path_for_storage("").unwrap(), "");
    assert_eq!(
        normalize_path_for_storage("~/foo/bar").unwrap(),
        "~/foo/bar"
    );
    assert_eq!(
        normalize_path_for_storage("@config/app.json").unwrap(),
        "@config/app.json"
    );
}

#[test]
fn test_round_trip_home_path() {
    let home = dirs::home_dir().expect("home dir");
    let target = home.join("some").join("nested").join("file.txt");
    let target_str = target.to_string_lossy();

    let normalized = normalize_path_for_storage(&target_str).unwrap();
    assert!(
        normalized.starts_with("~/"),
        "expected normalized path to start with ~/, got: {normalized}"
    );
    assert_eq!(normalized, "~/some/nested/file.txt");

    let expanded = expand_path(&normalized).unwrap();
    assert_eq!(expanded, target);
}

#[test]
fn test_expand_unfolded_path() {
    let un_folded = "/tmp/custom/legacy/path";
    let expanded = expand_path(un_folded).unwrap();
    assert_eq!(expanded, PathBuf::from(un_folded));
}
