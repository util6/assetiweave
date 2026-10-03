use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationCardRenderer {
    Markdown,
    Plain,
    Path,
    Json,
    Code,
    Command,
    TerminalOutput,
    Diff,
    CompactAction,
}

impl ConversationCardRenderer {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Plain => "plain",
            Self::Path => "path",
            Self::Json => "json",
            Self::Code => "code",
            Self::Command => "command",
            Self::TerminalOutput => "terminal_output",
            Self::Diff => "diff",
            Self::CompactAction => "compact_action",
        }
    }
}
