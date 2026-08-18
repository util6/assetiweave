use super::prelude::*;

pub(super) struct DetectionCtx<'a> {
    pub(super) source: &'a Source,
    pub(super) path: &'a Path,
    pub(super) relative_path: &'a str,
    pub(super) format: AssetFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Detection {
    pub(super) kind: AssetKind,
    pub(super) confidence: u8,
}

pub(super) trait AssetDetector: Send + Sync {
    fn id(&self) -> &'static str;
    fn version(&self) -> u32;
    fn priority(&self) -> i32;
    fn detect(&self, ctx: &DetectionCtx<'_>) -> Option<Detection>;
}

struct NameDetector {
    id: &'static str,
    kind: AssetKind,
    priority: i32,
    tokens: &'static [&'static str],
}

impl AssetDetector for NameDetector {
    fn id(&self) -> &'static str {
        self.id
    }

    fn version(&self) -> u32 {
        1
    }

    fn priority(&self) -> i32 {
        self.priority
    }

    fn detect(&self, ctx: &DetectionCtx<'_>) -> Option<Detection> {
        let lower = ctx.relative_path.to_lowercase();
        self.tokens
            .iter()
            .any(|token| lower.contains(token))
            .then_some(Detection {
                kind: self.kind,
                confidence: 100,
            })
    }
}

struct McpDetector;

impl AssetDetector for McpDetector {
    fn id(&self) -> &'static str {
        "builtin.mcp"
    }

    fn version(&self) -> u32 {
        1
    }

    fn priority(&self) -> i32 {
        40
    }

    fn detect(&self, ctx: &DetectionCtx<'_>) -> Option<Detection> {
        let lower = ctx.relative_path.to_lowercase();
        matches!(
            ctx.format,
            AssetFormat::Json | AssetFormat::Yaml | AssetFormat::Toml
        )
        .then_some(())
        .filter(|_| lower.contains("mcp"))
        .map(|_| Detection {
            kind: AssetKind::Mcp,
            confidence: 100,
        })
    }
}

static PROMPT: NameDetector = NameDetector {
    id: "builtin.prompt",
    kind: AssetKind::Prompt,
    priority: 100,
    tokens: &["prompt"],
};
static RULE: NameDetector = NameDetector {
    id: "builtin.rule",
    kind: AssetKind::Rule,
    priority: 90,
    tokens: &["rule", ".cursorrules", "requirements", "design"],
};
static MEMORY: NameDetector = NameDetector {
    id: "builtin.memory",
    kind: AssetKind::Memory,
    priority: 80,
    tokens: &["memory"],
};
static AGENT: NameDetector = NameDetector {
    id: "builtin.agent",
    kind: AssetKind::Agent,
    priority: 70,
    tokens: &["agent"],
};
static WORKFLOW: NameDetector = NameDetector {
    id: "builtin.workflow",
    kind: AssetKind::Workflow,
    priority: 60,
    tokens: &["workflow"],
};
static COMMAND: NameDetector = NameDetector {
    id: "builtin.command",
    kind: AssetKind::Command,
    priority: 50,
    tokens: &["command", "slash"],
};
static MCP: McpDetector = McpDetector;

pub(super) fn detectors() -> &'static [&'static dyn AssetDetector] {
    static DETECTORS: [&'static dyn AssetDetector; 7] =
        [&PROMPT, &RULE, &MEMORY, &AGENT, &WORKFLOW, &COMMAND, &MCP];
    &DETECTORS
}

pub(super) fn detect(ctx: &DetectionCtx<'_>) -> Option<(&'static str, u32, Detection)> {
    detectors()
        .iter()
        .filter_map(|detector| {
            detector.detect(ctx).map(|detection| {
                (
                    (*detector).priority(),
                    detection.confidence,
                    *detector,
                    detection,
                )
            })
        })
        .max_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then(left.1.cmp(&right.1))
                .then_with(|| right.2.id().cmp(left.2.id()))
        })
        .map(|(_, _, detector, detection)| (detector.id(), detector.version(), detection))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::models::{SourceKind, SourceOrigin};
    use std::path::PathBuf;

    #[test]
    fn detector_order_is_stable_and_priority_driven() {
        let source = Source {
            id: "source".into(),
            name: "source".into(),
            kind: SourceKind::Local,
            root_path: "/tmp".into(),
            scanner_kind: SourceScannerKind::Mixed,
            source_origin: SourceOrigin::LocalFolder,
            repo_root: None,
            scan_root: "/tmp".into(),
            origin_app_kind: None,
            origin_provider_id: None,
            include_globs: vec![],
            exclude_globs: vec![],
            default_kind: None,
            enabled: true,
            priority: 0,
            last_scanned_at: None,
            last_scan_status: None,
        };
        let ctx = DetectionCtx {
            source: &source,
            path: &PathBuf::from("/tmp/prompt-rule.md"),
            relative_path: "prompt-rule.md",
            format: AssetFormat::Markdown,
        };
        assert_eq!(
            detect(&ctx).map(|(_, _, result)| result.kind),
            Some(AssetKind::Prompt)
        );
    }
}
