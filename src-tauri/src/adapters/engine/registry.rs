use super::protocol;
use crate::backend::{application::AppService, dto::AppResult};
use schemars::{generate::SchemaSettings, JsonSchema};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CommandRisk {
    Read,
    Write,
    HighRiskWrite,
}

impl CommandRisk {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::HighRiskWrite => "high-risk-write",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CommandExposure {
    Friendly,
    App,
    System,
}

impl CommandExposure {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Friendly => "friendly",
            Self::App => "app",
            Self::System => "system",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ParamSpec {
    name: &'static str,
    description: &'static str,
    aliases: &'static [&'static str],
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CommandSpec {
    pub(crate) method: &'static str,
    pub(crate) canonical_method: &'static str,
    pub(crate) description: &'static str,
    pub(crate) risk: CommandRisk,
    pub(crate) exposure: CommandExposure,
    pub(crate) supports_dry_run: bool,
    params: &'static [ParamSpec],
    params_schema: fn() -> Value,
    validate_typed_params: fn(&Value) -> Result<(), String>,
    handler: fn(Value) -> DispatchResult,
    cli: Option<&'static str>,
    since: &'static str,
    deprecated: bool,
}

impl CommandSpec {
    pub(crate) fn dispatch(&self, params: Value) -> DispatchResult {
        (self.handler)(params)
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct NoParams {}

#[derive(Debug, Deserialize, JsonSchema)]
#[allow(dead_code)]
struct SchemaGetParams {
    method: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RevealPathParams {
    path: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ParamViolation {
    param: String,
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actual: Option<String>,
}

pub(crate) type DispatchResult = Result<Value, DispatchFailure>;

#[derive(Debug)]
pub(crate) enum DispatchFailure {
    InvalidParams(String),
    OpenService(String),
    App(String),
    Serialize(String),
}

macro_rules! param {
    ($name:literal, $description:literal) => {
        ParamSpec {
            name: $name,
            description: $description,
            aliases: &[],
        }
    };
    ($name:literal, $description:literal, [$($alias:literal),+]) => {
        ParamSpec {
            name: $name,
            description: $description,
            aliases: &[$($alias),+],
        }
    };
}

macro_rules! command {
    (
        $method:literal,
        $canonical:literal,
        $description:literal,
        $risk:ident,
        $exposure:ident,
        $dry_run:expr,
        $params_type:ty,
        Service => |$service:ident, $typed_params:ident| $handler:expr,
        $params:expr,
        $cli:expr
        $(, since: $since:literal, deprecated: $deprecated:expr)?
    ) => {
        command!(@build
            method: $method,
            canonical_method: $canonical,
            description: $description,
            risk: CommandRisk::$risk,
            exposure: CommandExposure::$exposure,
            supports_dry_run: $dry_run,
            params_type: $params_type,
            params: $params,
            handler: command!(@service_handler $exposure,
                |params| dispatch_service(
                    params,
                    |$service: &AppService, $typed_params: $params_type| $handler,
                )
            ),
            cli: $cli,
            since: command!(@since $($since)?),
            deprecated: command!(@deprecated $($deprecated)?),
        )
    };
    (
        $method:literal,
        $canonical:literal,
        $description:literal,
        $risk:ident,
        $exposure:ident,
        $dry_run:expr,
        $params_type:ty,
        System => |$typed_params:ident| $handler:expr,
        $params:expr,
        $cli:expr
        $(, since: $since:literal, deprecated: $deprecated:expr)?
    ) => {
        command!(@build
            method: $method,
            canonical_method: $canonical,
            description: $description,
            risk: CommandRisk::$risk,
            exposure: CommandExposure::$exposure,
            supports_dry_run: $dry_run,
            params_type: $params_type,
            params: $params,
            handler: command!(@system_handler $exposure,
                |params| dispatch_system(
                    params,
                    |$typed_params: $params_type| $handler,
                )
            ),
            cli: $cli,
            since: command!(@since $($since)?),
            deprecated: command!(@deprecated $($deprecated)?),
        )
    };
    (@build
        method: $method:expr,
        canonical_method: $canonical:expr,
        description: $description:expr,
        risk: $risk:expr,
        exposure: $exposure:expr,
        supports_dry_run: $dry_run:expr,
        params_type: $params_type:ty,
        params: $params:expr,
        handler: $handler:expr,
        cli: $cli:expr,
        since: $since:expr,
        deprecated: $deprecated:expr,
    ) => {
        CommandSpec {
            method: $method,
            canonical_method: $canonical,
            description: $description,
            risk: $risk,
            exposure: $exposure,
            supports_dry_run: $dry_run,
            params: $params,
            params_schema: params_schema_for::<$params_type>,
            validate_typed_params: validate_typed_params::<$params_type>,
            handler: $handler,
            cli: $cli,
            since: $since,
            deprecated: $deprecated,
        }
    };
    (@since) => {
        "0.1.0"
    };
    (@since $since:literal) => {
        $since
    };
    (@deprecated) => {
        false
    };
    (@deprecated $deprecated:expr) => {
        $deprecated
    };
    (@service_handler Friendly, $handler:expr) => {
        $handler
    };
    (@service_handler App, $handler:expr) => {
        $handler
    };
    (@system_handler System, $handler:expr) => {
        $handler
    };
    (@system_handler App, $handler:expr) => {
        $handler
    };
}

const COMMAND_SPECS: &[CommandSpec] = &[
    command!(
        "overview.get",
        "overview.get",
        "Get the AssetIWeave overview",
        Read,
        Friendly,
        false,
        NoParams,
        Service => |service, _params| service.overview(),
        &[],
        Some("assetiweave-cli overview")
    ),
    command!(
        "source.list",
        "source.list",
        "List registered asset sources",
        Read,
        Friendly,
        false,
        NoParams,
        Service => |service, _params| service.list_sources(),
        &[],
        Some("assetiweave-cli source list")
    ),
    command!(
        "source.add",
        "source.add",
        "Add a source with CLI-oriented options",
        Write,
        Friendly,
        true,
        crate::backend::application::SourceAddParams,
        Service => |service, params| service.add_source_with_options(params),
        &[
            param!("name", "Source display name"),
            param!("kind", "Source kind"),
            param!("root_path", "Source root directory", ["rootPath"]),
            param!("scanner_kind", "Scanner kind", ["scannerKind"]),
            param!("source_origin", "Source origin", ["sourceOrigin"]),
            param!("include_globs", "Include glob patterns", ["includeGlobs"]),
            param!("exclude_globs", "Exclude glob patterns", ["excludeGlobs"]),
            param!("default_kind", "Default asset kind", ["defaultKind"]),
            param!("enabled", "Whether the source is enabled"),
            param!("priority", "Source priority"),
            param!("repo_root", "Repository root", ["repoRoot"]),
            param!("scan_root", "Relative scan root", ["scanRoot"]),
            param!(
                "origin_app_kind",
                "Origin application kind",
                ["originAppKind"]
            ),
            param!("dry_run", "Preview without persisting", ["dryRun"]),
        ],
        Some("assetiweave-cli source add --name <name> --path <path>")
    ),
    command!(
        "source.remove",
        "source.remove",
        "Remove a source registration",
        HighRiskWrite,
        Friendly,
        true,
        crate::backend::application::SourceRemoveParams,
        Service => |service, params| service.remove_source(params),
        &[
            param!("id", "Source identifier"),
            param!("dry_run", "Preview without removing", ["dryRun"]),
            param!("yes", "Confirm the destructive operation"),
        ],
        Some("assetiweave-cli source remove <source-id> --yes")
    ),
    command!(
        "source.scan",
        "source.scan",
        "Scan registered sources",
        Write,
        Friendly,
        true,
        crate::backend::application::SourceScanParams,
        Service => |service, params| service.scan_sources(params),
        &[
            param!("kind", "Optional asset kind filter"),
            param!(
                "dry_run",
                "Return current assets without scanning",
                ["dryRun"]
            ),
        ],
        Some("assetiweave-cli source scan")
    ),
    command!(
        "profile.list",
        "profile.list",
        "List target profiles",
        Read,
        Friendly,
        false,
        NoParams,
        Service => |service, _params| service.list_profiles(),
        &[],
        Some("assetiweave-cli profile list")
    ),
    command!(
        "asset.list",
        "asset.list",
        "List catalog assets",
        Read,
        Friendly,
        false,
        crate::backend::application::ListAssetsParams,
        Service => |service, params| service.list_assets(params),
        &[param!("kind", "Optional asset kind filter")],
        Some("assetiweave-cli asset list")
    ),
    command!(
        "skill.list",
        "skill.list",
        "List Skill assets",
        Read,
        Friendly,
        false,
        NoParams,
        Service => |service, _params| service.list_skills(),
        &[],
        Some("assetiweave-cli skill list")
    ),
    command!(
        "skill.import",
        "skill.import",
        "Import a Skill into the AssetIWeave backup library",
        Write,
        Friendly,
        true,
        crate::backend::application::ImportSkillParams,
        Service => |service, params| service.import_skill(params),
        &[
            param!("from", "Directory containing SKILL.md"),
            param!("name", "Optional imported Skill name"),
            param!("dry_run", "Preview without copying", ["dryRun"]),
        ],
        Some("assetiweave-cli skill import --from <dir>")
    ),
    command!(
        "skill.search",
        "skill.search",
        "Search internet providers for Skill candidates",
        Read,
        Friendly,
        false,
        crate::backend::application::SkillSearchParams,
        Service => |service, params| service.search_skills(params),
        &[
            param!("query", "Skill search query"),
            param!("provider", "Search provider"),
            param!("limit", "Maximum candidate count"),
        ],
        Some("assetiweave-cli skill search --query <query>")
    ),
    command!(
        "skill.acquire",
        "skill.acquire",
        "Download and import a Skill candidate",
        HighRiskWrite,
        Friendly,
        true,
        crate::backend::application::SkillAcquireParams,
        Service => |service, params| service.acquire_skill(params),
        &[
            param!("url", "GitHub repository or tree URL"),
            param!("branch", "Git branch override"),
            param!("path", "Skill directory path inside the repository"),
            param!("name", "Imported Skill name"),
            param!("dry_run", "Preview without cloning or importing", ["dryRun"]),
            param!("yes", "Confirm download and import"),
        ],
        Some("assetiweave-cli skill acquire --url <github-url> --yes")
    ),
    command!(
        "skill.remote.list",
        "skill.remote.list",
        "List acquired Skill remote sources",
        Read,
        Friendly,
        false,
        NoParams,
        Service => |service, _params| service.list_skill_remote_sources(),
        &[],
        Some("assetiweave-cli skill remote list")
    ),
    command!(
        "skill.remote.check",
        "skill.remote.check",
        "Check acquired Skill remote sources for drift",
        Write,
        Friendly,
        false,
        crate::backend::application::SkillRemoteCheckParams,
        Service => |service, params| service.check_skill_remote_sources(params),
        &[param!("asset_id", "Optional asset identifier", ["assetId"])],
        Some("assetiweave-cli skill remote check [asset-id]")
    ),
    command!(
        "skill.backup",
        "skill.backup",
        "Back up a Skill into the AssetIWeave library",
        Write,
        Friendly,
        false,
        crate::backend::application::RequiredAssetIdParams,
        Service => |service, params| service.backup_skill(params.asset_id),
        &[param!("asset_id", "Asset identifier", ["assetId"])],
        Some("assetiweave-cli skill backup <asset-id>")
    ),
    command!(
        "skill.delete",
        "skill.delete",
        "Delete an AssetIWeave backup-library Skill",
        HighRiskWrite,
        Friendly,
        true,
        crate::backend::application::AssetRefParams,
        Service => |service, params| service.delete_skill(params),
        &[
            param!("asset_ref", "Asset identifier or name", ["assetRef"]),
            param!("profile_id", "Optional target profile", ["profileId"]),
            param!("dry_run", "Preview without deleting", ["dryRun"]),
            param!("yes", "Confirm the destructive operation"),
            param!("unmount", "Unmount managed targets before deleting"),
        ],
        Some("assetiweave-cli skill delete <asset-ref> --yes")
    ),
    command!(
        "skill.mount",
        "skill.mount",
        "Mount a Skill to a target profile",
        Write,
        Friendly,
        true,
        crate::backend::application::AssetRefParams,
        Service => |service, params| service.mount_skill(params, true),
        &[
            param!("asset_ref", "Asset identifier or name", ["assetRef"]),
            param!("profile_id", "Target profile identifier", ["profileId"]),
            param!("dry_run", "Preview without mounting", ["dryRun"]),
        ],
        Some("assetiweave-cli skill mount <asset-ref> --profile <profile-id>")
    ),
    command!(
        "skill.unmount",
        "skill.unmount",
        "Unmount a Skill from a target profile",
        Write,
        Friendly,
        true,
        crate::backend::application::AssetRefParams,
        Service => |service, params| service.mount_skill(params, false),
        &[
            param!("asset_ref", "Asset identifier or name", ["assetRef"]),
            param!("profile_id", "Target profile identifier", ["profileId"]),
            param!("dry_run", "Preview without unmounting", ["dryRun"]),
        ],
        Some("assetiweave-cli skill unmount <asset-ref> --profile <profile-id>")
    ),
    command!(
        "skill.group.list",
        "skill.group.list",
        "List Skill groups",
        Read,
        Friendly,
        false,
        NoParams,
        Service => |service, _params| service.list_skill_groups(),
        &[],
        Some("assetiweave-cli skill group list")
    ),
    command!(
        "skill.group.get",
        "skill.group.get",
        "Show a Skill group with resolved members",
        Read,
        Friendly,
        false,
        crate::backend::application::GroupIdParams,
        Service => |service, params| service.get_skill_group(params.group_id),
        &[param!("group_id", "Skill group identifier", ["groupId"])],
        Some("assetiweave-cli skill group show <group-id>")
    ),
    command!(
        "skill.group.create",
        "skill.group.create",
        "Create a Skill group",
        Write,
        Friendly,
        false,
        crate::backend::application::CreateSkillGroupParams,
        Service => |service, params| service.create_skill_group(params.input),
        &[param!("input", "Skill group input")],
        Some("assetiweave-cli skill group create --name <name>")
    ),
    command!(
        "skill.group.update",
        "skill.group.update",
        "Update a Skill group",
        Write,
        Friendly,
        false,
        crate::backend::application::UpdateSkillGroupParams,
        Service => |service, params| service.update_skill_group(params.group),
        &[param!("group", "Complete Skill group record")],
        Some("assetiweave-cli skill group update <group-id> --json <json>")
    ),
    command!(
        "skill.group.delete",
        "skill.group.delete",
        "Delete a Skill group",
        HighRiskWrite,
        Friendly,
        false,
        crate::backend::application::GroupIdParams,
        Service => |service, params| service.delete_skill_group(params.group_id),
        &[param!("group_id", "Skill group identifier", ["groupId"])],
        Some("assetiweave-cli skill group delete <group-id> --yes")
    ),
    command!(
        "skill.group.members.set",
        "skill.group.members.set",
        "Replace the manual members of a Skill group",
        Write,
        Friendly,
        false,
        crate::backend::application::SetSkillGroupManualMembersParams,
        Service => |service, params| service.set_skill_group_manual_members(params.group_id, params.asset_ids),
        &[
            param!("group_id", "Skill group identifier", ["groupId"]),
            param!("asset_ids", "Manual member asset identifiers", ["assetIds"]),
        ],
        Some("assetiweave-cli skill group members set <group-id> --asset <asset-id>")
    ),
    command!(
        "skill.group.mount",
        "skill.group.mount",
        "Mount a Skill group to a target profile",
        Write,
        Friendly,
        true,
        crate::backend::application::SkillGroupMountParams,
        Service => |service, params| service.mount_skill_group(params, true),
        &[
            param!("group_id", "Skill group identifier", ["groupId"]),
            param!("profile_id", "Target profile identifier", ["profileId"]),
            param!("dry_run", "Preview without mounting", ["dryRun"]),
        ],
        Some("assetiweave-cli skill group mount <group-id> --profile <profile-id>")
    ),
    command!(
        "skill.group.unmount",
        "skill.group.unmount",
        "Unmount a Skill group from a target profile",
        HighRiskWrite,
        Friendly,
        true,
        crate::backend::application::SkillGroupMountParams,
        Service => |service, params| service.mount_skill_group(params, false),
        &[
            param!("group_id", "Skill group identifier", ["groupId"]),
            param!("profile_id", "Target profile identifier", ["profileId"]),
            param!("dry_run", "Preview without unmounting", ["dryRun"]),
            param!("yes", "Confirm the destructive operation"),
        ],
        Some("assetiweave-cli skill group unmount <group-id> --profile <profile-id> --yes")
    ),
    command!(
        "skill.group.exclusive.preview",
        "skill.group.exclusive.preview",
        "Preview an exclusive Skill group mount operation",
        Read,
        Friendly,
        false,
        crate::backend::application::SkillGroupExclusiveMountParams,
        Service => |service, params| service.preview_skill_group_exclusive_mount(params.input),
        &[param!("input", "Exclusive mount input")],
        Some("assetiweave-cli skill group exclusive preview --group <group-id> --profile <profile-id>")
    ),
    command!(
        "skill.group.exclusive.apply",
        "skill.group.exclusive.apply",
        "Apply an exclusive Skill group mount operation",
        HighRiskWrite,
        Friendly,
        false,
        crate::backend::application::SkillGroupExclusiveMountParams,
        Service => |service, params| service.apply_skill_group_exclusive_mount(params.input),
        &[param!("input", "Exclusive mount input")],
        Some("assetiweave-cli skill group exclusive apply --group <group-id> --profile <profile-id> --yes")
    ),
    command!(
        "conversation.adapter.list",
        "conversation.adapter.list",
        "List conversation adapters",
        Read,
        Friendly,
        false,
        NoParams,
        Service => |service, _params| service.list_conversation_adapters(),
        &[],
        Some("assetiweave-cli conversation adapter list")
    ),
    command!(
        "conversation.adapter.scaffold",
        "conversation.adapter.scaffold",
        "Create a language-neutral conversation adapter manifest scaffold",
        Write,
        Friendly,
        true,
        crate::backend::conversations::ExternalAdapterScaffoldParams,
        Service => |service, params| service.scaffold_conversation_adapter(params),
        &[
            param!("directory", "Directory where scaffold files will be created"),
            param!("id", "Adapter identifier"),
            param!("name", "Adapter display name"),
            param!("dry_run", "Preview without writing files", ["dryRun"]),
        ],
        Some("assetiweave-cli conversation adapter scaffold --directory <dir> --id <id> --name <name>")
    ),
    command!(
        "conversation.adapter.validate",
        "conversation.adapter.validate",
        "Validate a conversation adapter manifest",
        Read,
        Friendly,
        false,
        crate::backend::conversations::ExternalAdapterValidateParams,
        Service => |service, params| service.validate_conversation_adapter(params),
        &[param!("manifest_path", "Adapter manifest path", ["manifestPath"])],
        Some("assetiweave-cli conversation adapter validate <manifest>")
    ),
    command!(
        "conversation.adapter.register",
        "conversation.adapter.register",
        "Register a trusted conversation adapter script",
        HighRiskWrite,
        Friendly,
        true,
        crate::backend::conversations::ExternalAdapterRegisterParams,
        Service => |service, params| service.register_conversation_adapter(params),
        &[
            param!("manifest_path", "Adapter manifest path", ["manifestPath"]),
            param!("dry_run", "Preview without persisting", ["dryRun"]),
            param!("yes", "Confirm trusting this adapter"),
        ],
        Some("assetiweave-cli conversation adapter register <manifest> --yes")
    ),
    command!(
        "conversation.adapter.unregister",
        "conversation.adapter.unregister",
        "Unregister an external conversation adapter",
        HighRiskWrite,
        Friendly,
        true,
        crate::backend::application::ConversationAdapterUnregisterParams,
        Service => |service, params| service.unregister_conversation_adapter(params),
        &[
            param!("adapter_id", "Adapter identifier", ["adapterId"]),
            param!("dry_run", "Preview without unregistering", ["dryRun"]),
            param!("yes", "Confirm unregistering this adapter"),
        ],
        Some("assetiweave-cli conversation adapter unregister <adapter-id> --yes")
    ),
    command!(
        "conversation.adapter.try-run",
        "conversation.adapter.try-run",
        "Run a conversation adapter manifest once and validate its NDJSON output",
        HighRiskWrite,
        Friendly,
        false,
        crate::backend::conversations::ExternalAdapterTryRunParams,
        Service => |service, params| service.try_run_conversation_adapter(params),
        &[
            param!("manifest_path", "Adapter manifest path", ["manifestPath"]),
            param!("method", "Adapter method to run"),
            param!("location", "Source location"),
            param!("session_id", "Optional external session identifier", ["sessionId"]),
            param!("yes", "Confirm executing this adapter"),
        ],
        Some("assetiweave-cli conversation adapter try-run <manifest> --method read_session --yes")
    ),
    command!(
        "conversation.source.list",
        "conversation.source.list",
        "List conversation sources",
        Read,
        Friendly,
        false,
        NoParams,
        Service => |service, _params| service.list_conversation_sources(),
        &[],
        Some("assetiweave-cli conversation source list")
    ),
    command!(
        "conversation.source.add",
        "conversation.source.add",
        "Create or update a conversation source",
        Write,
        Friendly,
        true,
        crate::backend::application::ConversationSourceUpsertParams,
        Service => |service, params| service.upsert_conversation_source(params),
        &[
            param!("source", "Conversation source record"),
            param!("dry_run", "Preview without persisting", ["dryRun"]),
        ],
        Some("assetiweave-cli conversation source add --source-json <json>")
    ),
    command!(
        "conversation.source.update",
        "conversation.source.update",
        "Update a conversation source",
        Write,
        Friendly,
        true,
        crate::backend::application::ConversationSourceUpsertParams,
        Service => |service, params| service.upsert_conversation_source(params),
        &[
            param!("source", "Conversation source record"),
            param!("dry_run", "Preview without persisting", ["dryRun"]),
        ],
        Some("assetiweave-cli conversation source update --source-json <json>")
    ),
    command!(
        "conversation.source.disable",
        "conversation.source.disable",
        "Disable a conversation source",
        Write,
        Friendly,
        true,
        crate::backend::application::ConversationSourceDisableParams,
        Service => |service, params| service.disable_conversation_source(params),
        &[
            param!("id", "Conversation source identifier"),
            param!("dry_run", "Preview without disabling", ["dryRun"]),
        ],
        Some("assetiweave-cli conversation source disable <source-id>")
    ),
    command!(
        "conversation.sync",
        "conversation.sync",
        "Synchronize conversation sources",
        Write,
        Friendly,
        true,
        crate::backend::application::ConversationSyncParams,
        Service => |service, params| service.sync_conversations(params),
        &[
            param!("source_id", "Optional source identifier", ["sourceId"]),
            param!("adapter_id", "Optional adapter identifier", ["adapterId"]),
            param!("dry_run", "Preview without importing", ["dryRun"]),
        ],
        Some("assetiweave-cli conversation sync")
    ),
    command!(
        "conversation.session.list",
        "conversation.session.list",
        "List imported conversation sessions",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationSessionListParams,
        Service => |service, params| service.list_conversation_sessions(params),
        &[
            param!("adapter_id", "Optional adapter filter", ["adapterId"]),
            param!("source_id", "Optional source filter", ["sourceId"]),
            param!("query", "Search query"),
            param!("limit", "Maximum number of sessions"),
            param!("offset", "Pagination offset"),
        ],
        Some("assetiweave-cli conversation session list")
    ),
    command!(
        "conversation.search",
        "conversation.search",
        "Search conversation content cards",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationSearchParams,
        Service => |service, params| service.search_conversation_records(params),
        &[
            param!("record_kind", "Conversation record kind", ["recordKind"]),
            param!("adapter_id", "Optional adapter filter", ["adapterId"]),
            param!("source_id", "Optional source filter", ["sourceId"]),
            param!("query", "Search query"),
            param!("content_types", "Content card types", ["contentTypes"]),
            param!("limit", "Maximum number of hits"),
            param!("offset", "Pagination offset"),
        ],
        None
    ),
    command!(
        "conversation.session.get",
        "conversation.session.get",
        "Get one conversation session with question groups",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationSessionGetParams,
        Service => |service, params| service.get_conversation_session(params),
        &[param!("session_id", "Session identifier", ["sessionId"])],
        Some("assetiweave-cli conversation session get <session-id>")
    ),
    command!(
        "conversation.session.export",
        "conversation.session.export",
        "Export one conversation session as Markdown",
        Write,
        Friendly,
        true,
        crate::backend::application::ConversationSessionExportParams,
        Service => |service, params| service.export_conversation_session(params),
        &[
            param!("session_id", "Session identifier", ["sessionId"]),
            param!("output_root", "Output root directory", ["outputRoot"]),
            param!(
                "question_ids",
                "Optional question identifiers to export instead of the full session",
                ["questionIds"]
            ),
            param!(
                "content_filter",
                "Optional content categories to include in Markdown export",
                ["contentFilter"]
            ),
            param!("dry_run", "Preview without writing", ["dryRun"]),
        ],
        Some("assetiweave-cli conversation session export <session-id> --output-root <dir>")
    ),
    command!(
        "conversation.web-record.list",
        "conversation.web-record.list",
        "List imported web conversation records",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationSessionListParams,
        Service => |service, params| service.list_web_record_sessions(params),
        &[
            param!("adapter_id", "Optional adapter filter", ["adapterId"]),
            param!("source_id", "Optional source filter", ["sourceId"]),
            param!("query", "Search query"),
            param!("limit", "Maximum number of web records"),
            param!("offset", "Pagination offset"),
        ],
        Some("assetiweave-cli conversation web-record list")
    ),
    command!(
        "conversation.web-record.get",
        "conversation.web-record.get",
        "Get one web conversation record with question groups",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationSessionGetParams,
        Service => |service, params| service.get_web_record_session(params),
        &[param!("session_id", "Web record identifier", ["sessionId"])],
        Some("assetiweave-cli conversation web-record get <record-id>")
    ),
    command!(
        "conversation.web-record.export",
        "conversation.web-record.export",
        "Export one web conversation record as Markdown",
        Write,
        Friendly,
        true,
        crate::backend::application::ConversationSessionExportParams,
        Service => |service, params| service.export_web_record_session(params),
        &[
            param!("session_id", "Web record identifier", ["sessionId"]),
            param!("output_root", "Output root directory", ["outputRoot"]),
            param!(
                "question_ids",
                "Optional question identifiers to export instead of the full record",
                ["questionIds"]
            ),
            param!(
                "content_filter",
                "Optional content categories to include in Markdown export",
                ["contentFilter"]
            ),
            param!("dry_run", "Preview without writing", ["dryRun"]),
        ],
        Some("assetiweave-cli conversation web-record export <record-id> --output-root <dir>")
    ),
    command!(
        "conversation.question.list",
        "conversation.question.list",
        "List question groups in a conversation session",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationQuestionListParams,
        Service => |service, params| service.list_conversation_questions(params),
        &[
            param!("session_id", "Session identifier", ["sessionId"]),
            param!("query", "Search query"),
            param!("limit", "Maximum number of questions"),
            param!("offset", "Pagination offset"),
        ],
        Some("assetiweave-cli conversation question list <session-id>")
    ),
    command!(
        "conversation.question.get",
        "conversation.question.get",
        "Get one conversation question group",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationQuestionGetParams,
        Service => |service, params| service.get_conversation_question(params),
        &[param!("question_id", "Question identifier", ["questionId"])],
        Some("assetiweave-cli conversation question get <question-id>")
    ),
    command!(
        "conversation.question.merge",
        "conversation.question.merge",
        "Merge adjacent conversation question groups",
        Write,
        Friendly,
        true,
        crate::backend::application::ConversationQuestionMergeParams,
        Service => |service, params| service.merge_conversation_questions(params),
        &[
            param!("question_ids", "Adjacent question identifiers in session order", ["questionIds"]),
            param!("dry_run", "Preview without merging", ["dryRun"]),
        ],
        Some("assetiweave-cli conversation question merge <question-id>...")
    ),
    command!(
        "conversation.question.split",
        "conversation.question.split",
        "Split a conversation question group before a turn",
        Write,
        Friendly,
        true,
        crate::backend::application::ConversationQuestionSplitParams,
        Service => |service, params| service.split_conversation_question(params),
        &[
            param!("question_id", "Question identifier", ["questionId"]),
            param!("before_turn_id", "Turn identifier that starts the new question", ["beforeTurnId"]),
            param!("dry_run", "Preview without splitting", ["dryRun"]),
        ],
        Some("assetiweave-cli conversation question split <question-id> --before-turn <turn-id>")
    ),
    command!(
        "doctor.run",
        "doctor.run",
        "Run local AssetIWeave diagnostics",
        Read,
        Friendly,
        false,
        NoParams,
        Service => |service, _params| service.run_doctor(),
        &[],
        Some("assetiweave-cli doctor")
    ),
    command!(
        "system.version",
        "system.version",
        "Show Engine protocol and contract versions",
        Read,
        System,
        false,
        NoParams,
        System => |_params| protocol::version_info(),
        &[],
        Some("assetiweave-cli version")
    ),
    command!(
        "schema.list",
        "schema.list",
        "List engine command contracts",
        Read,
        System,
        false,
        NoParams,
        System => |_params| schema_index(),
        &[],
        Some("assetiweave-cli schema")
    ),
    command!(
        "schema.get",
        "schema.get",
        "Get one engine command contract",
        Read,
        System,
        false,
        SchemaGetParams,
        System => |params| params.method.as_deref().map_or_else(schema_index, schema_get),
        &[param!("method", "Method name to inspect")],
        Some("assetiweave-cli schema <method>")
    ),
    command!(
        "get_app_overview",
        "overview.get",
        "Get the AssetIWeave overview",
        Read,
        App,
        false,
        NoParams,
        Service => |service, _params| service.overview(),
        &[],
        None
    ),
    command!(
        "get_app_settings",
        "settings.get",
        "Get the user settings JSON file and managed settings paths",
        Read,
        App,
        false,
        NoParams,
        Service => |service, _params| service.get_app_settings(),
        &[],
        Some("assetiweave-cli settings show")
    ),
    command!(
        "save_app_settings",
        "settings.save",
        "Save the user settings JSON file",
        Write,
        App,
        false,
        crate::backend::application::SaveAppSettingsParams,
        Service => |service, params| service.save_app_settings(Value::Object(params.settings.into_iter().collect())),
        &[param!("settings", "Normalized application settings object")],
        Some("assetiweave-cli settings save --json <json>")
    ),
    command!(
        "list_assets",
        "asset.list",
        "List catalog assets",
        Read,
        App,
        false,
        crate::backend::application::ListAssetsParams,
        Service => |service, params| service.list_assets(params),
        &[param!("kind", "Optional asset kind filter")],
        None
    ),
    command!(
        "get_skill_backup_settings",
        "get_skill_backup_settings",
        "Get Skill backup library settings",
        Read,
        App,
        false,
        NoParams,
        Service => |service, _params| service.get_skill_backup_settings(),
        &[],
        None
    ),
    command!(
        "update_skill_backup_settings",
        "update_skill_backup_settings",
        "Update and optionally migrate the Skill backup library",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::UpdateSkillBackupSettingsParams,
        Service => |service, params| service.update_skill_backup_settings(params),
        &[
            param!("root_path", "Backup library root path", ["rootPath"]),
            param!("migrate", "Migrate existing backup files"),
        ],
        None
    ),
    command!(
        "backup_skill",
        "skill.backup",
        "Back up a Skill into the AssetIWeave library",
        Write,
        App,
        false,
        crate::backend::application::RequiredAssetIdParams,
        Service => |service, params| service.backup_skill(params.asset_id),
        &[param!("asset_id", "Asset identifier", ["assetId"])],
        None
    ),
    command!(
        "search_skills",
        "skill.search",
        "Search internet providers for Skill candidates",
        Read,
        App,
        false,
        crate::backend::application::SkillSearchParams,
        Service => |service, params| service.search_skills(params),
        &[
            param!("query", "Skill search query"),
            param!("provider", "Search provider"),
            param!("limit", "Maximum candidate count"),
        ],
        Some("assetiweave-cli skill search --query <query>")
    ),
    command!(
        "acquire_skill",
        "skill.acquire",
        "Download and import a Skill candidate",
        HighRiskWrite,
        App,
        true,
        crate::backend::application::SkillAcquireParams,
        Service => |service, params| service.acquire_skill(params),
        &[
            param!("url", "GitHub repository or tree URL"),
            param!("branch", "Git branch override"),
            param!("path", "Skill directory path inside the repository"),
            param!("name", "Imported Skill name"),
            param!("dry_run", "Preview without cloning or importing", ["dryRun"]),
            param!("yes", "Confirm download and import"),
        ],
        Some("assetiweave-cli skill acquire --url <github-url> --yes")
    ),
    command!(
        "list_skill_remote_sources",
        "skill.remote.list",
        "List acquired Skill remote sources",
        Read,
        App,
        false,
        NoParams,
        Service => |service, _params| service.list_skill_remote_sources(),
        &[],
        Some("assetiweave-cli skill remote list")
    ),
    command!(
        "check_skill_remote_sources",
        "skill.remote.check",
        "Check acquired Skill remote sources for drift",
        Write,
        App,
        false,
        crate::backend::application::SkillRemoteCheckParams,
        Service => |service, params| service.check_skill_remote_sources(params),
        &[param!("asset_id", "Optional asset identifier", ["assetId"])],
        Some("assetiweave-cli skill remote check [asset-id]")
    ),
    command!(
        "list_sources",
        "source.list",
        "List registered asset sources",
        Read,
        App,
        false,
        NoParams,
        Service => |service, _params| service.list_sources(),
        &[],
        None
    ),
    command!(
        "list_skill_sources",
        "list_skill_sources",
        "List registered Skill sources",
        Read,
        App,
        false,
        NoParams,
        Service => |service, _params| service.list_skill_sources(),
        &[],
        None
    ),
    command!(
        "create_source",
        "create_source",
        "Create an asset source",
        Write,
        App,
        false,
        crate::backend::application::CreateSourceParams,
        Service => |service, params| service.add_source(params.source),
        &[param!("source", "Source input")],
        None
    ),
    command!(
        "update_source",
        "update_source",
        "Update an asset source",
        Write,
        App,
        false,
        crate::backend::application::UpdateSourceParams,
        Service => |service, params| service.update_source(params.source),
        &[param!("source", "Complete source record")],
        None
    ),
    command!(
        "delete_source",
        "source.remove",
        "Delete an asset source registration",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::IdParams,
        Service => |service, params| service.delete_source(params.id),
        &[param!("id", "Source identifier")],
        None
    ),
    command!(
        "update_asset_description",
        "update_asset_description",
        "Update an asset description",
        Write,
        App,
        false,
        crate::backend::application::UpdateAssetDescriptionParams,
        Service => |service, params| service.update_asset_description(params.asset_id, params.description),
        &[
            param!("asset_id", "Asset identifier", ["assetId"]),
            param!("description", "New description"),
        ],
        None
    ),
    command!(
        "delete_asset",
        "skill.delete",
        "Delete an AssetIWeave-managed asset",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::DeleteAssetParams,
        Service => |service, params| service.delete_asset(params.asset_id, params.unmount),
        &[
            param!("asset_id", "Asset identifier", ["assetId"]),
            param!("unmount", "Unmount managed targets before deleting"),
        ],
        None
    ),
    command!(
        "list_profiles",
        "profile.list",
        "List target profiles",
        Read,
        App,
        false,
        NoParams,
        Service => |service, _params| service.list_profiles(),
        &[],
        None
    ),
    command!(
        "create_profile",
        "create_profile",
        "Create a target profile",
        Write,
        App,
        false,
        crate::backend::application::CreateProfileParams,
        Service => |service, params| service.create_profile(params.input),
        &[param!("input", "Target profile input")],
        None
    ),
    command!(
        "update_profile",
        "update_profile",
        "Update a target profile",
        Write,
        App,
        false,
        crate::backend::application::UpdateProfileParams,
        Service => |service, params| service.update_profile(params.profile),
        &[param!("profile", "Complete target profile record")],
        None
    ),
    command!(
        "delete_profile",
        "delete_profile",
        "Delete a target profile",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::IdParams,
        Service => |service, params| service.delete_profile(params.id),
        &[param!("id", "Target profile identifier")],
        None
    ),
    command!(
        "get_navigation_model",
        "get_navigation_model",
        "Get the navigation model",
        Read,
        App,
        false,
        NoParams,
        Service => |service, _params| service.navigation_model(),
        &[],
        None
    ),
    command!(
        "update_navigation_model",
        "update_navigation_model",
        "Update the navigation model",
        Write,
        App,
        false,
        crate::backend::application::UpdateNavigationModelParams,
        Service => |service, params| service.update_navigation_model(params.model),
        &[param!("model", "Navigation model")],
        None
    ),
    command!(
        "list_app_shortcuts",
        "list_app_shortcuts",
        "List enabled App shortcuts",
        Read,
        App,
        false,
        NoParams,
        Service => |service, _params| service.list_app_shortcuts(),
        &[],
        None
    ),
    command!(
        "list_app_shortcut_settings",
        "list_app_shortcut_settings",
        "List App shortcut settings",
        Read,
        App,
        false,
        NoParams,
        Service => |service, _params| service.list_app_shortcut_settings(),
        &[],
        None
    ),
    command!(
        "update_app_shortcuts",
        "update_app_shortcuts",
        "Update App shortcut settings",
        Write,
        App,
        false,
        crate::backend::application::UpdateAppShortcutsParams,
        Service => |service, params| service.update_app_shortcuts(params.shortcuts),
        &[param!("shortcuts", "App shortcut records")],
        None
    ),
    command!(
        "list_asset_mounts",
        "list_asset_mounts",
        "List requested asset mounts",
        Read,
        App,
        false,
        crate::backend::application::AssetIdParams,
        Service => |service, params| service.list_asset_mounts(params.asset_id.as_deref()),
        &[param!("asset_id", "Optional asset identifier", ["assetId"])],
        None
    ),
    command!(
        "list_asset_mount_statuses",
        "list_asset_mount_statuses",
        "Inspect physical asset mount statuses",
        Read,
        App,
        false,
        crate::backend::application::AssetIdParams,
        Service => |service, params| service.list_asset_mount_statuses(params.asset_id.as_deref()),
        &[param!("asset_id", "Optional asset identifier", ["assetId"])],
        None
    ),
    command!(
        "refresh_asset_mount_statuses",
        "refresh_asset_mount_statuses",
        "Refresh physical asset mount observations",
        Write,
        App,
        false,
        crate::backend::application::AssetIdParams,
        Service => |service, params| service.refresh_asset_mount_statuses(params.asset_id.as_deref()),
        &[param!("asset_id", "Optional asset identifier", ["assetId"])],
        None
    ),
    command!(
        "list_skill_groups",
        "skill.group.list",
        "List Skill groups",
        Read,
        App,
        false,
        NoParams,
        Service => |service, _params| service.list_skill_groups(),
        &[],
        None
    ),
    command!(
        "create_skill_group",
        "create_skill_group",
        "Create a Skill group",
        Write,
        App,
        false,
        crate::backend::application::CreateSkillGroupParams,
        Service => |service, params| service.create_skill_group(params.input),
        &[param!("input", "Skill group input")],
        None
    ),
    command!(
        "update_skill_group",
        "update_skill_group",
        "Update a Skill group",
        Write,
        App,
        false,
        crate::backend::application::UpdateSkillGroupParams,
        Service => |service, params| service.update_skill_group(params.group),
        &[param!("group", "Complete Skill group record")],
        None
    ),
    command!(
        "delete_skill_group",
        "delete_skill_group",
        "Delete a Skill group",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::GroupIdParams,
        Service => |service, params| service.delete_skill_group(params.group_id),
        &[param!("group_id", "Skill group identifier", ["groupId"])],
        None
    ),
    command!(
        "set_skill_group_manual_members",
        "set_skill_group_manual_members",
        "Replace the manual members of a Skill group",
        Write,
        App,
        false,
        crate::backend::application::SetSkillGroupManualMembersParams,
        Service => |service, params| service.set_skill_group_manual_members(params.group_id, params.asset_ids),
        &[
            param!("group_id", "Skill group identifier", ["groupId"]),
            param!("asset_ids", "Manual member asset identifiers", ["assetIds"]),
        ],
        None
    ),
    command!(
        "apply_skill_group_mount",
        "apply_skill_group_mount",
        "Apply a Skill group mount state",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::ApplySkillGroupMountParams,
        Service => |service, params| service.apply_skill_group_mount(&params.group_id, &params.profile_id, params.enabled),
        &[
            param!("group_id", "Skill group identifier", ["groupId"]),
            param!("profile_id", "Target profile identifier", ["profileId"]),
            param!("enabled", "Requested mount state"),
        ],
        None
    ),
    command!(
        "preview_skill_group_exclusive_mount",
        "preview_skill_group_exclusive_mount",
        "Preview an exclusive Skill group mount operation",
        Read,
        App,
        false,
        crate::backend::application::SkillGroupExclusiveMountParams,
        Service => |service, params| service.preview_skill_group_exclusive_mount(params.input),
        &[param!("input", "Exclusive mount input")],
        None
    ),
    command!(
        "apply_skill_group_exclusive_mount",
        "apply_skill_group_exclusive_mount",
        "Apply an exclusive Skill group mount operation",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::SkillGroupExclusiveMountParams,
        Service => |service, params| service.apply_skill_group_exclusive_mount(params.input),
        &[param!("input", "Exclusive mount input")],
        None
    ),
    command!(
        "toggle_asset_mount",
        "toggle_asset_mount",
        "Toggle an asset mount using physical state",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::AssetProfileParams,
        Service => |service, params| service.toggle_asset_mount(&params.asset_id, &params.profile_id),
        &[
            param!("asset_id", "Asset identifier", ["assetId"]),
            param!("profile_id", "Target profile identifier", ["profileId"]),
        ],
        None
    ),
    command!(
        "mount_asset_mount",
        "mount_asset_mount",
        "Mount an asset to a target profile",
        Write,
        App,
        false,
        crate::backend::application::AssetProfileParams,
        Service => |service, params| service.mount_asset_by_id(&params.asset_id, &params.profile_id),
        &[
            param!("asset_id", "Asset identifier", ["assetId"]),
            param!("profile_id", "Target profile identifier", ["profileId"]),
        ],
        None
    ),
    command!(
        "unmount_asset_mount",
        "unmount_asset_mount",
        "Unmount an asset from a target profile",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::AssetProfileParams,
        Service => |service, params| service.unmount_asset_by_id(&params.asset_id, &params.profile_id),
        &[
            param!("asset_id", "Asset identifier", ["assetId"]),
            param!("profile_id", "Target profile identifier", ["profileId"]),
        ],
        None
    ),
    command!(
        "set_asset_mount",
        "set_asset_mount",
        "Set an asset mount state",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::SetAssetMountParams,
        Service => |service, params| service.set_asset_mount(&params.asset_id, &params.profile_id, params.enabled, params.strategy),
        &[
            param!("asset_id", "Asset identifier", ["assetId"]),
            param!("profile_id", "Target profile identifier", ["profileId"]),
            param!("enabled", "Requested mount state"),
            param!("strategy", "Optional deployment strategy"),
        ],
        None
    ),
    command!(
        "scan_sources",
        "source.scan",
        "Scan registered sources",
        Write,
        App,
        true,
        crate::backend::application::SourceScanParams,
        Service => |service, params| service.scan_sources(params),
        &[
            param!("kind", "Optional asset kind filter"),
            param!(
                "dry_run",
                "Return current assets without scanning",
                ["dryRun"]
            ),
        ],
        None
    ),
    command!(
        "scan_skill_sources",
        "scan_skill_sources",
        "Scan registered Skill sources",
        Write,
        App,
        false,
        NoParams,
        Service => |service, _params| service.scan_skill_sources(),
        &[],
        None
    ),
    command!(
        "list_conversation_adapters",
        "conversation.adapter.list",
        "List conversation adapters",
        Read,
        App,
        false,
        NoParams,
        Service => |service, _params| service.list_conversation_adapters(),
        &[],
        None
    ),
    command!(
        "scaffold_conversation_adapter",
        "conversation.adapter.scaffold",
        "Create a language-neutral conversation adapter manifest scaffold",
        Write,
        App,
        false,
        crate::backend::conversations::ExternalAdapterScaffoldParams,
        Service => |service, params| service.scaffold_conversation_adapter(params),
        &[
            param!("directory", "Directory where scaffold files will be created"),
            param!("id", "Adapter identifier"),
            param!("name", "Adapter display name"),
            param!("dry_run", "Preview without writing files", ["dryRun"]),
        ],
        None
    ),
    command!(
        "validate_conversation_adapter",
        "conversation.adapter.validate",
        "Validate a conversation adapter manifest",
        Read,
        App,
        false,
        crate::backend::conversations::ExternalAdapterValidateParams,
        Service => |service, params| service.validate_conversation_adapter(params),
        &[param!("manifest_path", "Adapter manifest path", ["manifestPath"])],
        None
    ),
    command!(
        "register_conversation_adapter",
        "conversation.adapter.register",
        "Register a trusted conversation adapter script",
        HighRiskWrite,
        App,
        false,
        crate::backend::conversations::ExternalAdapterRegisterParams,
        Service => |service, params| service.register_conversation_adapter(params),
        &[
            param!("manifest_path", "Adapter manifest path", ["manifestPath"]),
            param!("dry_run", "Preview without persisting", ["dryRun"]),
            param!("yes", "Confirm trusting this adapter"),
        ],
        None
    ),
    command!(
        "unregister_conversation_adapter",
        "conversation.adapter.unregister",
        "Unregister an external conversation adapter",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::ConversationAdapterUnregisterParams,
        Service => |service, params| service.unregister_conversation_adapter(params),
        &[
            param!("adapter_id", "Adapter identifier", ["adapterId"]),
            param!("dry_run", "Preview without unregistering", ["dryRun"]),
            param!("yes", "Confirm unregistering this adapter"),
        ],
        None
    ),
    command!(
        "try_run_conversation_adapter",
        "conversation.adapter.try-run",
        "Run a conversation adapter manifest once and validate its NDJSON output",
        HighRiskWrite,
        App,
        false,
        crate::backend::conversations::ExternalAdapterTryRunParams,
        Service => |service, params| service.try_run_conversation_adapter(params),
        &[
            param!("manifest_path", "Adapter manifest path", ["manifestPath"]),
            param!("method", "Adapter method to run"),
            param!("location", "Source location"),
            param!("session_id", "Optional external session identifier", ["sessionId"]),
            param!("yes", "Confirm executing this adapter"),
        ],
        None
    ),
    command!(
        "list_conversation_sources",
        "conversation.source.list",
        "List conversation sources",
        Read,
        App,
        false,
        NoParams,
        Service => |service, _params| service.list_conversation_sources(),
        &[],
        None
    ),
    command!(
        "upsert_conversation_source",
        "conversation.source.add",
        "Create or update a conversation source",
        Write,
        App,
        false,
        crate::backend::application::ConversationSourceUpsertParams,
        Service => |service, params| service.upsert_conversation_source(params),
        &[
            param!("source", "Conversation source record"),
            param!("dry_run", "Preview without persisting", ["dryRun"]),
        ],
        None
    ),
    command!(
        "disable_conversation_source",
        "conversation.source.disable",
        "Disable a conversation source",
        Write,
        App,
        false,
        crate::backend::application::ConversationSourceDisableParams,
        Service => |service, params| service.disable_conversation_source(params),
        &[
            param!("id", "Conversation source identifier"),
            param!("dry_run", "Preview without disabling", ["dryRun"]),
        ],
        None
    ),
    command!(
        "sync_conversations",
        "conversation.sync",
        "Synchronize conversation sources",
        Write,
        App,
        false,
        crate::backend::application::ConversationSyncParams,
        Service => |service, params| service.sync_conversations(params),
        &[
            param!("source_id", "Optional source identifier", ["sourceId"]),
            param!("adapter_id", "Optional adapter identifier", ["adapterId"]),
            param!("dry_run", "Preview without importing", ["dryRun"]),
        ],
        None
    ),
    command!(
        "get_conversation_sync_task",
        "get_conversation_sync_task",
        "Get the current desktop conversation sync background task",
        Read,
        App,
        false,
        NoParams,
        System => |_params| Value::Null,
        &[],
        None
    ),
    command!(
        "list_conversation_sessions",
        "conversation.session.list",
        "List imported conversation sessions",
        Read,
        App,
        false,
        crate::backend::application::ConversationSessionListParams,
        Service => |service, params| service.list_conversation_sessions(params),
        &[
            param!("adapter_id", "Optional adapter filter", ["adapterId"]),
            param!("source_id", "Optional source filter", ["sourceId"]),
            param!("query", "Search query"),
            param!("limit", "Maximum number of sessions"),
            param!("offset", "Pagination offset"),
        ],
        None
    ),
    command!(
        "get_conversation_session",
        "conversation.session.get",
        "Get one conversation session with question groups",
        Read,
        App,
        false,
        crate::backend::application::ConversationSessionGetParams,
        Service => |service, params| service.get_conversation_session(params),
        &[param!("session_id", "Session identifier", ["sessionId"])],
        None
    ),
    command!(
        "export_conversation_session",
        "conversation.session.export",
        "Export one conversation session as Markdown",
        Write,
        App,
        false,
        crate::backend::application::ConversationSessionExportParams,
        Service => |service, params| service.export_conversation_session(params),
        &[
            param!("session_id", "Session identifier", ["sessionId"]),
            param!("output_root", "Output root directory", ["outputRoot"]),
            param!(
                "question_ids",
                "Optional question identifiers to export instead of the full session",
                ["questionIds"]
            ),
            param!(
                "content_filter",
                "Optional content categories to include in Markdown export",
                ["contentFilter"]
            ),
            param!("dry_run", "Preview without writing", ["dryRun"]),
        ],
        None
    ),
    command!(
        "list_web_record_sessions",
        "conversation.web-record.list",
        "List imported web conversation records",
        Read,
        App,
        false,
        crate::backend::application::ConversationSessionListParams,
        Service => |service, params| service.list_web_record_sessions(params),
        &[
            param!("adapter_id", "Optional adapter filter", ["adapterId"]),
            param!("source_id", "Optional source filter", ["sourceId"]),
            param!("query", "Search query"),
            param!("limit", "Maximum number of web records"),
            param!("offset", "Pagination offset"),
        ],
        None
    ),
    command!(
        "get_web_record_session",
        "conversation.web-record.get",
        "Get one web conversation record with question groups",
        Read,
        App,
        false,
        crate::backend::application::ConversationSessionGetParams,
        Service => |service, params| service.get_web_record_session(params),
        &[param!("session_id", "Web record identifier", ["sessionId"])],
        None
    ),
    command!(
        "search_conversation_records",
        "conversation.search",
        "Search conversation content cards",
        Read,
        App,
        false,
        crate::backend::application::ConversationSearchParams,
        Service => |service, params| service.search_conversation_records(params),
        &[
            param!("record_kind", "Conversation record kind", ["recordKind"]),
            param!("adapter_id", "Optional adapter filter", ["adapterId"]),
            param!("source_id", "Optional source filter", ["sourceId"]),
            param!("query", "Search query"),
            param!("content_types", "Content card types", ["contentTypes"]),
            param!("limit", "Maximum number of hits"),
            param!("offset", "Pagination offset"),
        ],
        None
    ),
    command!(
        "export_web_record_session",
        "conversation.web-record.export",
        "Export one web conversation record as Markdown",
        Write,
        App,
        false,
        crate::backend::application::ConversationSessionExportParams,
        Service => |service, params| service.export_web_record_session(params),
        &[
            param!("session_id", "Web record identifier", ["sessionId"]),
            param!("output_root", "Output root directory", ["outputRoot"]),
            param!(
                "question_ids",
                "Optional question identifiers to export instead of the full record",
                ["questionIds"]
            ),
            param!(
                "content_filter",
                "Optional content categories to include in Markdown export",
                ["contentFilter"]
            ),
            param!("dry_run", "Preview without writing", ["dryRun"]),
        ],
        None
    ),
    command!(
        "list_conversation_questions",
        "conversation.question.list",
        "List question groups in a conversation session",
        Read,
        App,
        false,
        crate::backend::application::ConversationQuestionListParams,
        Service => |service, params| service.list_conversation_questions(params),
        &[
            param!("session_id", "Session identifier", ["sessionId"]),
            param!("query", "Search query"),
            param!("limit", "Maximum number of questions"),
            param!("offset", "Pagination offset"),
        ],
        None
    ),
    command!(
        "get_conversation_question",
        "conversation.question.get",
        "Get one conversation question group",
        Read,
        App,
        false,
        crate::backend::application::ConversationQuestionGetParams,
        Service => |service, params| service.get_conversation_question(params),
        &[param!("question_id", "Question identifier", ["questionId"])],
        None
    ),
    command!(
        "merge_conversation_questions",
        "conversation.question.merge",
        "Merge adjacent conversation question groups",
        Write,
        App,
        false,
        crate::backend::application::ConversationQuestionMergeParams,
        Service => |service, params| service.merge_conversation_questions(params),
        &[
            param!("question_ids", "Adjacent question identifiers in session order", ["questionIds"]),
            param!("dry_run", "Preview without merging", ["dryRun"]),
        ],
        None
    ),
    command!(
        "split_conversation_question",
        "conversation.question.split",
        "Split a conversation question group before a turn",
        Write,
        App,
        false,
        crate::backend::application::ConversationQuestionSplitParams,
        Service => |service, params| service.split_conversation_question(params),
        &[
            param!("question_id", "Question identifier", ["questionId"]),
            param!("before_turn_id", "Turn identifier that starts the new question", ["beforeTurnId"]),
            param!("dry_run", "Preview without splitting", ["dryRun"]),
        ],
        None
    ),
    command!(
        "create_plan",
        "create_plan",
        "Create a deployment plan",
        Read,
        App,
        false,
        crate::backend::application::ProfileIdParams,
        Service => |service, params| service.create_plan(params.profile_id.as_deref()),
        &[param!(
            "profile_id",
            "Optional target profile identifier",
            ["profileId"]
        )],
        None
    ),
    command!(
        "execute_plan",
        "execute_plan",
        "Execute deployment plan actions",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::ExecutePlanParams,
        Service => |service, params| service.execute_plan(params.plan, params.action_ids),
        &[
            param!("plan", "Deployment plan"),
            param!(
                "action_ids",
                "Optional selected action identifiers",
                ["actionIds"]
            ),
        ],
        None
    ),
    command!(
        "logs_get_snapshot",
        "logs_get_snapshot",
        "Read an AssetIWeave log snapshot",
        Read,
        App,
        false,
        crate::backend::application::LogsGetSnapshotParams,
        Service => |service, params| service.logs_get_snapshot(params.file_name, params.line_limit),
        &[
            param!("file_name", "Optional log file name", ["fileName"]),
            param!("line_limit", "Maximum line count", ["lineLimit"]),
        ],
        None
    ),
    command!(
        "logs_open_log_directory",
        "logs_open_log_directory",
        "Open the AssetIWeave log directory",
        Read,
        App,
        false,
        NoParams,
        Service => |service, _params| service.logs_open_log_directory(),
        &[],
        None
    ),
    command!(
        "logs_write_operation",
        "logs_write_operation",
        "Write a structured AssetIWeave operation log",
        Write,
        App,
        false,
        crate::backend::application::LogsWriteOperationParams,
        Service => |service, params| service.logs_write_operation(params.level, params.operation, params.message, params.fields),
        &[
            param!("level", "Log level"),
            param!("operation", "Operation name"),
            param!("message", "Log message"),
            param!("fields", "Optional structured fields"),
        ],
        None
    ),
    command!(
        "reveal_path",
        "reveal_path",
        "Reveal a local path in the system file manager",
        Read,
        App,
        false,
        RevealPathParams,
        System => |params| crate::adapters::platform::reveal_path(params.path),
        &[param!("path", "Local path to reveal")],
        None
    ),
];

pub(crate) fn command_specs() -> &'static [CommandSpec] {
    COMMAND_SPECS
}

pub(crate) fn find(method: &str) -> Option<&'static CommandSpec> {
    COMMAND_SPECS.iter().find(|spec| spec.method == method)
}

#[cfg(test)]
pub(crate) fn is_app_method(method: &str) -> bool {
    find(method).is_some_and(|spec| spec.exposure == CommandExposure::App)
}

pub(crate) fn requires_confirmation(spec: &CommandSpec, params: &Value) -> bool {
    spec.risk == CommandRisk::HighRiskWrite
        && !(spec.supports_dry_run
            && params
                .get("dry_run")
                .or_else(|| params.get("dryRun"))
                .and_then(Value::as_bool)
                .unwrap_or(false))
        && !params.get("yes").and_then(Value::as_bool).unwrap_or(false)
}

pub(crate) fn validate_params(
    spec: &CommandSpec,
    params: &Value,
) -> Result<Value, Vec<ParamViolation>> {
    let Some(object) = params.as_object() else {
        return Err(vec![ParamViolation {
            param: "$".to_string(),
            code: "expected_object",
            message: "method params must be a JSON object".to_string(),
            expected: Some("object".to_string()),
            actual: Some(value_kind(params).to_string()),
        }]);
    };

    let schema = contract_params_schema(spec);
    let properties = schema["properties"]
        .as_object()
        .expect("command params schema properties");
    let required = schema["required"]
        .as_array()
        .expect("command params schema required");
    let mut violations = Vec::new();
    for name in object.keys() {
        if !properties.contains_key(name)
            && find_param(spec, name).is_none()
            && !(spec.risk == CommandRisk::HighRiskWrite && name == "yes")
        {
            violations.push(ParamViolation {
                param: name.clone(),
                code: "unknown_param",
                message: format!("unknown parameter: {name}"),
                expected: None,
                actual: None,
            });
        }
    }

    for (name, property_schema) in properties {
        let aliases = find_param(spec, name).map_or(&[][..], |param| param.aliases);
        let present = std::iter::once(name.as_str())
            .chain(aliases.iter().copied())
            .filter_map(|name| object.get(name).map(|value| (name, value)))
            .collect::<Vec<_>>();
        if present.is_empty() {
            if required.contains(&json!(name)) {
                violations.push(ParamViolation {
                    param: name.clone(),
                    code: "required",
                    message: format!("missing required parameter: {name}"),
                    expected: schema_type(property_schema),
                    actual: None,
                });
            }
            continue;
        }
        if present.len() > 1 {
            violations.push(ParamViolation {
                param: name.clone(),
                code: "duplicate_alias",
                message: format!(
                    "parameter {} was provided more than once using aliases: {}",
                    name,
                    present
                        .iter()
                        .map(|(name, _)| *name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                expected: None,
                actual: None,
            });
            continue;
        }
        let (provided_name, value) = present[0];
        if !value_matches_schema(value, property_schema) {
            violations.push(ParamViolation {
                param: provided_name.to_string(),
                code: "invalid_type",
                message: format!("invalid value for parameter: {provided_name}"),
                expected: schema_type(property_schema),
                actual: Some(value_kind(value).to_string()),
            });
        }
    }

    if !violations.is_empty() {
        return Err(violations);
    }

    let normalized = normalize_aliases(spec, object);
    if let Err(message) = (spec.validate_typed_params)(&normalized) {
        Err(vec![ParamViolation {
            param: "$".to_string(),
            code: "invalid_value",
            message,
            expected: None,
            actual: None,
        }])
    } else {
        Ok(normalized)
    }
}

fn normalize_aliases(spec: &CommandSpec, object: &serde_json::Map<String, Value>) -> Value {
    let mut normalized = object.clone();
    for param in spec.params {
        for alias in param.aliases {
            if let Some(value) = normalized.remove(*alias) {
                normalized.insert(param.name.to_string(), value);
            }
        }
    }
    Value::Object(normalized)
}

fn find_param<'a>(spec: &'a CommandSpec, name: &str) -> Option<&'a ParamSpec> {
    spec.params
        .iter()
        .find(|param| param.name == name || param.aliases.contains(&name))
}

fn value_matches_schema(value: &Value, schema: &Value) -> bool {
    let type_matches = schema["type"].as_str().map_or_else(
        || {
            schema["type"].as_array().is_some_and(|types| {
                types
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|kind| value_matches_type(value, kind))
            })
        },
        |kind| value_matches_type(value, kind),
    );
    type_matches
        && schema["enum"].as_array().is_none_or(|values| {
            value.is_null() || values.iter().any(|candidate| candidate == value)
        })
}

fn value_matches_type(value: &Value, kind: &str) -> bool {
    match kind {
        "null" => value.is_null(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "integer" => value
            .as_number()
            .is_some_and(|number| number.is_i64() || number.is_u64()),
        "number" => value.is_number(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        _ => false,
    }
}

fn schema_type(schema: &Value) -> Option<String> {
    schema["type"].as_str().map(str::to_string).or_else(|| {
        schema["type"].as_array().map(|types| {
            types
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("|")
        })
    })
}

fn params_schema_for<T: JsonSchema>() -> Value {
    let generator = SchemaSettings::draft2020_12()
        .for_deserialize()
        .with(|settings| {
            settings.inline_subschemas = true;
            settings.meta_schema = None;
        })
        .into_generator();
    let mut schema = serde_json::to_value(generator.into_root_schema_for::<T>())
        .expect("serialize params schema");
    normalize_root_schema(&mut schema);
    schema
}

fn validate_typed_params<T: DeserializeOwned>(params: &Value) -> Result<(), String> {
    serde_json::from_value::<T>(params.clone())
        .map(|_| ())
        .map_err(|error| format!("params do not match the Rust request type: {error}"))
}

fn dispatch_service<P, T>(
    params: Value,
    handler: fn(&AppService, P) -> AppResult<T>,
) -> DispatchResult
where
    P: DeserializeOwned,
    T: Serialize,
{
    let params = deserialize_dispatch_params(params)?;
    let service = AppService::open_for_engine().map_err(DispatchFailure::OpenService)?;
    serialize_dispatch_result(handler(&service, params).map_err(DispatchFailure::App)?)
}

fn dispatch_system<P, T>(params: Value, handler: fn(P) -> T) -> DispatchResult
where
    P: DeserializeOwned,
    T: Serialize,
{
    let params = deserialize_dispatch_params(params)?;
    serialize_dispatch_result(handler(params))
}

fn deserialize_dispatch_params<T: DeserializeOwned>(params: Value) -> Result<T, DispatchFailure> {
    serde_json::from_value(params).map_err(|error| {
        DispatchFailure::InvalidParams(format!(
            "registered handler params failed after contract validation: {error}"
        ))
    })
}

fn serialize_dispatch_result<T: Serialize>(value: T) -> DispatchResult {
    serde_json::to_value(value).map_err(|error| DispatchFailure::Serialize(error.to_string()))
}

fn normalize_root_schema(schema: &mut Value) {
    let object = schema
        .as_object_mut()
        .expect("params schema must be an object");
    object.remove("$schema");
    object.remove("title");
    object.remove("$defs");
    object.insert("type".to_string(), json!("object"));
    object.insert("additionalProperties".to_string(), json!(false));
    object
        .entry("required".to_string())
        .or_insert_with(|| json!([]));
    object
        .entry("properties".to_string())
        .or_insert_with(|| json!({}));
    if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
        for property in properties.values_mut() {
            normalize_optional_property(property);
            if let Some(values) = property.get_mut("enum").and_then(Value::as_array_mut) {
                values.retain(|value| !value.is_null());
            }
        }
    }
}

fn normalize_optional_property(property: &mut Value) {
    let Some(any_of) = property.get("anyOf").and_then(Value::as_array) else {
        return;
    };
    let Some(non_null) = any_of
        .iter()
        .find(|candidate| candidate["type"] != json!("null"))
        .cloned()
    else {
        return;
    };
    let Some(base_type) = non_null["type"].as_str().map(str::to_string) else {
        return;
    };
    let mut normalized = non_null;
    let object = normalized.as_object_mut().expect("property schema");
    object.insert("type".to_string(), json!([base_type, "null"]));
    if let Some(values) = object.get_mut("enum").and_then(Value::as_array_mut) {
        values.retain(|value| !value.is_null());
    }
    *property = normalized;
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

pub(crate) fn schema_index() -> Value {
    let methods = command_specs()
        .iter()
        .map(|spec| spec.method)
        .collect::<Vec<_>>();
    let commands = command_specs()
        .iter()
        .map(command_contract)
        .collect::<Vec<_>>();
    json!({
        "protocol_version": protocol::PROTOCOL_VERSION,
        "contract_version": protocol::CONTRACT_VERSION,
        "engine_version": env!("CARGO_PKG_VERSION"),
        "methods": methods,
        "commands": commands
    })
}

pub(crate) fn schema_get(method: &str) -> Value {
    find(method).map_or_else(
        || {
            json!({
                "contract_version": protocol::CONTRACT_VERSION,
                "method": method,
                "known": false,
                "params_schema": params_schema_for::<NoParams>()
            })
        },
        command_contract,
    )
}

fn command_contract(spec: &CommandSpec) -> Value {
    json!({
        "contract_version": protocol::CONTRACT_VERSION,
        "method": spec.method,
        "canonical_method": spec.canonical_method,
        "description": spec.description,
        "risk": spec.risk,
        "confirmation_required": spec.risk == CommandRisk::HighRiskWrite,
        "exposure": spec.exposure,
        "supports_dry_run": spec.supports_dry_run,
        "params_schema": contract_params_schema(spec),
        "cli": spec.cli,
        "since": spec.since,
        "deprecated": spec.deprecated
    })
}

fn contract_params_schema(spec: &CommandSpec) -> Value {
    let mut schema = (spec.params_schema)();
    let properties = schema["properties"]
        .as_object_mut()
        .expect("command params schema properties");
    for param in spec.params {
        let property = properties.get_mut(param.name).unwrap_or_else(|| {
            panic!(
                "documented command parameter {}.{} is missing from its Rust request type",
                spec.method, param.name
            )
        });
        let object = property
            .as_object_mut()
            .expect("command property schema must be an object");
        object.insert("description".to_string(), json!(param.description));
        if !param.aliases.is_empty() {
            object.insert("aliases".to_string(), json!(param.aliases));
        }
    }
    for (name, property) in properties.iter_mut() {
        property
            .as_object_mut()
            .expect("command property schema must be an object")
            .entry("description".to_string())
            .or_insert_with(|| json!(name.replace('_', " ")));
    }
    if spec.risk == CommandRisk::HighRiskWrite && !properties.contains_key("yes") {
        properties.insert(
            "yes".to_string(),
            json!({
                "type": "boolean",
                "description": "Confirm the high-risk operation"
            }),
        );
    }
    schema
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn registry_methods_are_unique() {
        let methods = command_specs()
            .iter()
            .map(|spec| spec.method)
            .collect::<BTreeSet<_>>();
        assert_eq!(methods.len(), command_specs().len());
    }

    #[test]
    fn system_version_is_registered_as_system_command() {
        let spec = find("system.version").expect("system.version spec");
        assert_eq!(spec.exposure, CommandExposure::System);
        assert_eq!(spec.risk, CommandRisk::Read);
    }

    #[test]
    fn high_risk_schema_includes_confirmation() {
        let contract = schema_get("delete_source");
        assert_eq!(contract["risk"], json!("high-risk-write"));
        assert_eq!(
            contract["params_schema"]["properties"]["yes"]["type"],
            json!("boolean")
        );
    }

    #[test]
    fn dry_run_bypasses_high_risk_confirmation() {
        let spec = find("source.remove").expect("source.remove spec");
        assert!(!requires_confirmation(spec, &json!({ "dry_run": true })));
        assert!(requires_confirmation(spec, &json!({ "id": "source-id" })));
        assert!(!requires_confirmation(
            spec,
            &json!({ "id": "source-id", "yes": true })
        ));
    }

    #[test]
    fn unsupported_dry_run_cannot_bypass_high_risk_confirmation() {
        let spec = find("delete_source").expect("delete_source spec");
        assert!(!spec.supports_dry_run);
        assert!(requires_confirmation(
            spec,
            &json!({ "id": "source-id", "dry_run": true })
        ));
    }

    #[test]
    fn runtime_validation_accepts_aliases_and_rejects_unknown_or_invalid_values() {
        let spec = find("source.add").expect("source.add spec");
        let normalized = validate_params(
            spec,
            &json!({
                "name": "skills",
                "kind": "local",
                "rootPath": "/tmp/skills",
                "includeGlobs": [],
                "excludeGlobs": [],
                "enabled": true,
                "priority": 1
            }),
        )
        .expect("aliases should validate");
        assert_eq!(normalized["root_path"], json!("/tmp/skills"));
        assert!(normalized.get("rootPath").is_none());

        let violations = validate_params(
            spec,
            &json!({
                "name": "skills",
                "root_path": "/tmp/skills",
                "priority": "first",
                "typo": true
            }),
        )
        .expect_err("invalid params should fail");
        assert!(violations
            .iter()
            .any(|violation| violation.code == "unknown_param"));
        assert!(violations
            .iter()
            .any(|violation| violation.code == "invalid_type"));
    }

    #[test]
    fn runtime_validation_accepts_implicit_confirmation_param_once() {
        let spec = find("delete_source").expect("delete_source spec");
        assert!(validate_params(spec, &json!({ "id": "source-id", "yes": true })).is_ok());
    }

    #[test]
    fn source_add_contract_required_fields_match_deserialization_type() {
        let contract = schema_get("source.add");
        let required = contract["params_schema"]["required"]
            .as_array()
            .expect("source.add required fields");

        for field in [
            "name",
            "kind",
            "root_path",
            "include_globs",
            "exclude_globs",
            "enabled",
            "priority",
        ] {
            assert!(
                required.contains(&json!(field)),
                "source.add contract omitted required serde field {field}"
            );
        }
    }

    #[test]
    fn deployment_strategy_contract_matches_backend_model() {
        let contract = schema_get("set_asset_mount");
        assert_eq!(
            contract["params_schema"]["properties"]["strategy"]["enum"],
            json!([
                "symlink_to_source",
                "copy_to_target",
                "render",
                "append",
                "config_merge"
            ])
        );
    }

    #[test]
    fn committed_cli_contract_matches_registry() {
        let committed: Value = serde_json::from_str(include_str!(
            "../../../../cli/internal/schema/contract.json"
        ))
        .expect("parse committed CLI contract");
        assert_eq!(
            committed,
            schema_index(),
            "CLI contract drifted; run `pnpm cli:contract`"
        );
    }
}
