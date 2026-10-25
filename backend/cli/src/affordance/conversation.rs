use super::parser::AffordanceRegistry;
use std::collections::HashMap;
use std::sync::LazyLock;

const CONVERSATION_MARKDOWN: &str =
    include_str!("../../../../builtin-assets/affordance/conversation.md");

static CONVERSATION_REGISTRY: LazyLock<AffordanceRegistry> =
    LazyLock::new(|| AffordanceRegistry::parse(CONVERSATION_MARKDOWN));

static HELP_CACHE: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for (name, affordance) in CONVERSATION_REGISTRY.all_commands() {
        map.insert(name.clone(), affordance.render_after_long_help());
    }
    map
});

/// Returns the formatted after_long_help text for a conversation command or subcommand.
pub fn help(command: &str) -> &'static str {
    let key = command.trim().to_lowercase();
    if let Some(rendered) = HELP_CACHE.get(&key) {
        rendered.as_str()
    } else {
        let space_key = key.replace('.', " ");
        if let Some(rendered) = HELP_CACHE.get(&space_key) {
            rendered.as_str()
        } else {
            ""
        }
    }
}

/// Returns the conversation affordance registry for programmatic inspection.
pub fn registry() -> &'static AffordanceRegistry {
    &CONVERSATION_REGISTRY
}
