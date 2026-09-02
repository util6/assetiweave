use std::{fmt, future::Future, pin::Pin};

/// The amount of Provider history that was actually available to a replay.
///
/// `Simplified` means that the Provider only exposed its text-oriented
/// transcript. `Partial` is reserved for a damaged or bounded source; it is
/// intentionally not collapsed into `Unavailable` when some valid records can
/// still be shown.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum HistoryReplayFidelity {
    Full,
    Simplified,
    Partial,
    Unavailable,
}

impl HistoryReplayFidelity {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Simplified => "simplified",
            Self::Partial => "partial",
            Self::Unavailable => "unavailable",
        }
    }
}

impl fmt::Debug for HistoryReplayFidelity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The externally meaningful availability state of a replay.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum HistoryReplayStatus {
    Ready,
    Partial,
    Unavailable,
}

impl HistoryReplayStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Partial => "partial",
            Self::Unavailable => "unavailable",
        }
    }
}

impl fmt::Debug for HistoryReplayStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A Provider-neutral item read from history.
///
/// Tool entries are display facts only. They deliberately carry no input,
/// output, credential, or executable payload, so replay cannot be mistaken for
/// a new tool invocation.
#[derive(Clone, Eq, PartialEq)]
pub(crate) enum HistoryReplayEntry {
    UserMessage,
    AssistantText { text: String },
    ToolStart { item_id: String, name: String },
    ToolResult { item_id: String, success: bool },
    Notice { code: String },
}

impl fmt::Debug for HistoryReplayEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UserMessage => formatter.write_str("UserMessage"),
            Self::AssistantText { .. } => formatter.write_str("AssistantText(<redacted>)"),
            Self::ToolStart { item_id, name } => formatter
                .debug_struct("ToolStart")
                .field("item_id", item_id)
                .field("name", name)
                .finish(),
            Self::ToolResult { item_id, success } => formatter
                .debug_struct("ToolResult")
                .field("item_id", item_id)
                .field("success", success)
                .finish(),
            Self::Notice { code } => formatter
                .debug_struct("Notice")
                .field("code", code)
                .finish(),
        }
    }
}

/// Result returned by every Session Adapter's history replay path.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct HistoryReplayResult {
    pub(crate) text: String,
    pub(crate) fidelity: HistoryReplayFidelity,
    pub(crate) status: HistoryReplayStatus,
    pub(crate) entries: Vec<HistoryReplayEntry>,
}

impl HistoryReplayResult {
    pub(crate) fn new(
        text: String,
        fidelity: HistoryReplayFidelity,
        status: HistoryReplayStatus,
        entries: Vec<HistoryReplayEntry>,
    ) -> Self {
        Self {
            text,
            fidelity,
            status,
            entries,
        }
    }

    pub(crate) fn unavailable() -> Self {
        Self::new(
            String::new(),
            HistoryReplayFidelity::Unavailable,
            HistoryReplayStatus::Unavailable,
            Vec::new(),
        )
    }

    pub(crate) fn status_detail(&self) -> String {
        format!(
            "fidelity={};status={}",
            self.fidelity.as_str(),
            self.status.as_str()
        )
    }

    pub(crate) fn is_available(&self) -> bool {
        !matches!(self.status, HistoryReplayStatus::Unavailable)
    }
}

impl fmt::Debug for HistoryReplayResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HistoryReplayResult")
            .field("text", &"<redacted>")
            .field("fidelity", &self.fidelity)
            .field("status", &self.status)
            .field("entries", &self.entries)
            .finish()
    }
}

pub(crate) type HistoryReplayFuture<'a> =
    Pin<Box<dyn Future<Output = HistoryReplayResult> + Send + 'a>>;

/// Semantic history port shared by ACP and native Provider adapters.
pub(crate) trait HistoryReplayPort {
    fn replay<'a>(
        &'a mut self,
        provider_session_id: &'a str,
        max_bytes: usize,
    ) -> HistoryReplayFuture<'a>;
}
