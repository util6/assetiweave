use super::*;

#[test]
fn team_name_field_uses_validator_without_changing_whitespace_semantics() {
    use validator::Validate;
    let input = CreateTeamInput {
        id: None,
        name: "  ".into(),
        description: None,
        members: vec![],
    };
    let errors = input.validate().unwrap_err();
    assert!(errors.field_errors().contains_key("name"));
}
