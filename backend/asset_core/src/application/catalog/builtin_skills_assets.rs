pub(super) const ORGANIZER_SKILL_DIR: &str = "assetiweave-conversation-organizer";
pub(super) const ORGANIZER_SKILL: &[u8] = include_bytes!(
    "../../../../../builtin-assets/skills/assetiweave-conversation-organizer/SKILL.md"
);
pub(super) const ORGANIZER_MANIFEST: &[u8] = include_bytes!(
    "../../../../../builtin-assets/skills/assetiweave-conversation-organizer/assetiweave.skill.json"
);
pub(super) const ORGANIZER_ZCODE_MANIFEST: &[u8] =
    include_bytes!("../../../../../builtin-assets/adapters/zcode/conversation-adapter.json");
pub(super) const ORGANIZER_ZCODE_ADAPTER: &[u8] =
    include_bytes!("../../../../../builtin-assets/adapters/zcode/adapter.mjs");
pub(super) const ORGANIZER_ZCODE_SHELL_PROJECTOR: &[u8] =
    include_bytes!("../../../../../builtin-assets/adapters/zcode/shell-projector.cjs");
pub(super) const RECALL_SKILL: &[u8] =
    include_bytes!("../../../../../builtin-assets/skills/assetiweave-conversation-recall/SKILL.md");
pub(super) const RECALL_MANIFEST: &[u8] = include_bytes!(
    "../../../../../builtin-assets/skills/assetiweave-conversation-recall/assetiweave.skill.json"
);
pub(super) const WEB_REPAIR_SKILL: &[u8] = include_bytes!(
    "../../../../../builtin-assets/skills/assetiweave-web-conversation-repair/SKILL.md"
);
pub(super) const WEB_REPAIR_MANIFEST: &[u8] = include_bytes!(
    "../../../../../builtin-assets/skills/assetiweave-web-conversation-repair/assetiweave.skill.json"
);
pub(super) const MEMORY_SKILL: &[u8] =
    include_bytes!("../../../../../builtin-assets/skills/assetiweave-memory/SKILL.md");
pub(super) const MEMORY_MANIFEST: &[u8] = include_bytes!(
    "../../../../../builtin-assets/skills/assetiweave-memory/assetiweave.skill.json"
);
pub(super) const MEMORY_RECALL_SCRIPT: &[u8] =
    include_bytes!("../../../../../builtin-assets/skills/assetiweave-memory/scripts/recall.py");
pub(super) const MEMORY_GENERATION_SKILL: &[u8] =
    include_bytes!("../../../../../builtin-assets/skills/assetiweave-memory-generation/SKILL.md");
pub(super) const MEMORY_GENERATION_MANIFEST: &[u8] = include_bytes!(
    "../../../../../builtin-assets/skills/assetiweave-memory-generation/assetiweave.skill.json"
);

pub(super) struct EmbeddedFile {
    pub(super) relative_path: &'static str,
    pub(super) contents: &'static [u8],
    pub(super) executable: bool,
}

pub(super) const EMBEDDED_FILES: &[EmbeddedFile] = &[
    EmbeddedFile {
        relative_path: "assetiweave-conversation-organizer/SKILL.md",
        contents: ORGANIZER_SKILL,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-conversation-organizer/assetiweave.skill.json",
        contents: ORGANIZER_MANIFEST,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-conversation-organizer/scripts/zcode-conversation-adapter/conversation-adapter.json",
        contents: ORGANIZER_ZCODE_MANIFEST,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-conversation-organizer/scripts/zcode-conversation-adapter/adapter.mjs",
        contents: ORGANIZER_ZCODE_ADAPTER,
        executable: true,
    },
    EmbeddedFile {
        relative_path: "assetiweave-conversation-organizer/scripts/zcode-conversation-adapter/shell-projector.cjs",
        contents: ORGANIZER_ZCODE_SHELL_PROJECTOR,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-conversation-recall/SKILL.md",
        contents: RECALL_SKILL,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-conversation-recall/assetiweave.skill.json",
        contents: RECALL_MANIFEST,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-web-conversation-repair/SKILL.md",
        contents: WEB_REPAIR_SKILL,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-web-conversation-repair/assetiweave.skill.json",
        contents: WEB_REPAIR_MANIFEST,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-memory/SKILL.md",
        contents: MEMORY_SKILL,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-memory/assetiweave.skill.json",
        contents: MEMORY_MANIFEST,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-memory/scripts/recall.py",
        contents: MEMORY_RECALL_SCRIPT,
        executable: true,
    },
    EmbeddedFile {
        relative_path: "assetiweave-memory-generation/SKILL.md",
        contents: MEMORY_GENERATION_SKILL,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-memory-generation/assetiweave.skill.json",
        contents: MEMORY_GENERATION_MANIFEST,
        executable: false,
    },
];
