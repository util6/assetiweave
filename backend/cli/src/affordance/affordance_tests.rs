use super::conversation;
use super::parser::AffordanceRegistry;

const SAMPLE_MARKDOWN: &str = r#"
# demo

Demo top-level command description.

### Use this when
- Performing demo actions.
- Testing affordance parser.

### Avoid when
- In production without confirmation.

### Examples
- `aiwc demo run`
- `aiwc demo inspect --all`

### Related skills
- `aiwc skill builtin read assetiweave-memory`

### Risk
Read-only test command.

## subcmd

Subcommand description.

### Use this when
- Running sub operation.

### Avoid when
- Never avoid this.

### Examples
- `aiwc demo subcmd`

### Related skills
- `aiwc skill builtin read assetiweave-conversation-recall`

### Risk
Low: safe operation.
"#;

#[test]
fn test_parse_markdown_commands_and_sections() {
    let registry = AffordanceRegistry::parse(SAMPLE_MARKDOWN);

    let demo = registry.get("demo").expect("should find demo command");
    assert_eq!(demo.name, "demo");
    assert!(demo
        .description
        .as_deref()
        .unwrap()
        .contains("Demo top-level"));
    assert!(demo.use_this_when.contains("Performing demo actions."));
    assert!(demo
        .avoid_when
        .contains("In production without confirmation."));
    assert_eq!(demo.examples.len(), 2);
    assert_eq!(demo.related_skills.len(), 1);
    assert!(demo.related_skills[0].contains("assetiweave-memory"));
    assert!(demo.risk.contains("Read-only test command."));

    let subcmd = registry.get("subcmd").expect("should find subcmd");
    assert_eq!(subcmd.name, "subcmd");
    assert!(subcmd.use_this_when.contains("Running sub operation."));
    assert!(subcmd.avoid_when.contains("Never avoid this."));
    assert_eq!(subcmd.examples.len(), 1);
    assert!(subcmd.risk.contains("Low: safe operation."));
}

#[test]
fn test_render_after_long_help_formatting() {
    let registry = AffordanceRegistry::parse(SAMPLE_MARKDOWN);
    let demo = registry.get("demo").unwrap();
    let rendered = demo.render_after_long_help();

    assert!(rendered.contains("Affordance:"));
    assert!(rendered.contains("Use this when:"));
    assert!(rendered.contains("Avoid when:"));
    assert!(rendered.contains("Examples:"));
    assert!(rendered.contains("Related skills:"));
    assert!(rendered.contains("Risk:"));
    assert!(rendered.contains("aiwc demo run"));
    assert!(rendered.contains("aiwc skill builtin read assetiweave-memory"));
}

#[test]
fn test_missing_command_fallback() {
    let registry = AffordanceRegistry::parse(SAMPLE_MARKDOWN);

    // Direct get should return None
    assert!(registry.get("nonexistent").is_none());
    assert!(registry.render_help("nonexistent").is_none());

    // Fallback getter should produce safe default affordance
    let fallback = registry.get_or_fallback("nonexistent");
    assert_eq!(fallback.name, "nonexistent");
    assert!(!fallback.use_this_when.is_empty());
    assert!(!fallback.avoid_when.is_empty());
    assert!(!fallback.examples.is_empty());
    assert!(!fallback.risk.is_empty());

    let rendered = registry.render_help_or_fallback("nonexistent");
    assert!(rendered.contains("Affordance:"));
    assert!(rendered.contains("Use this when:"));
    assert!(rendered.contains("nonexistent"));
}

#[test]
fn test_conversation_builtin_affordance_integrity() {
    let registry = conversation::registry();
    let required_commands = [
        "conversation",
        "search",
        "list",
        "outline",
        "card",
        "sync",
        "adapter",
        "source",
        "data",
    ];

    for cmd in required_commands {
        let affordance = registry
            .get(cmd)
            .unwrap_or_else(|| panic!("missing affordance for `{}` in conversation.md", cmd));

        assert!(
            !affordance.use_this_when.trim().is_empty(),
            "use_this_when must not be empty for {}",
            cmd
        );
        assert!(
            !affordance.avoid_when.trim().is_empty(),
            "avoid_when must not be empty for {}",
            cmd
        );
        assert!(
            !affordance.examples.is_empty(),
            "examples must not be empty for {}",
            cmd
        );
        assert!(
            !affordance.related_skills.is_empty(),
            "related_skills must not be empty for {}",
            cmd
        );
        assert!(
            !affordance.risk.trim().is_empty(),
            "risk must not be empty for {}",
            cmd
        );

        let help = conversation::help(cmd);
        assert!(
            !help.is_empty(),
            "conversation::help(\"{}\") must not be empty",
            cmd
        );
        assert!(
            help.contains("Use this when"),
            "help for {} must contain 'Use this when'",
            cmd
        );
        assert!(
            help.contains("Avoid when"),
            "help for {} must contain 'Avoid when'",
            cmd
        );
        assert!(
            help.contains("Examples"),
            "help for {} must contain 'Examples'",
            cmd
        );
        assert!(
            help.contains("Related skills"),
            "help for {} must contain 'Related skills'",
            cmd
        );
        assert!(
            help.contains("Risk"),
            "help for {} must contain 'Risk'",
            cmd
        );
    }

    // Verify related skills reference
    let search_help = conversation::help("search");
    assert!(search_help.contains("aiwc skill builtin read assetiweave-conversation-recall"));
}
