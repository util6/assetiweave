//! Engine 命令注册表：Conversations / Adapters

use super::super::dispatch::*;
use super::super::types::*;
use crate::backend::application::AppService;
use crate::{command, param};
use serde_json::{json, Value};

pub(super) const COMMANDS: &[CommandSpec] = &[
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
        None
    ),
    command!(
        "conversation.adapter.scaffold",
        "conversation.adapter.scaffold",
        "Create a system-runtime conversation adapter manifest scaffold",
        Write,
        Friendly,
        true,
        crate::backend::infrastructure::conversations::ExternalAdapterScaffoldParams,
        Service => |service, params| service.scaffold_conversation_adapter(params),
        &[
            param!("directory", "Directory where scaffold files will be created"),
            param!("id", "Adapter identifier"),
            param!("name", "Adapter display name"),
            param!("runtime_type", "Adapter runtime: node, python, bash, or executable", ["runtimeType"]),
            param!("runtime_entry", "Adapter entry path relative to the adapter directory", ["runtimeEntry"]),
            param!("runtime_version", "Runtime version requirement such as >=20 or >=3.10", ["runtimeVersion"]),
            param!("dry_run", "Preview without writing files", ["dryRun"]),
        ],
        None
    ),
    command!(
        "conversation.adapter.validate",
        "conversation.adapter.validate",
        "Validate a conversation adapter manifest",
        Read,
        Friendly,
        false,
        crate::backend::infrastructure::conversations::ExternalAdapterValidateParams,
        Service => |service, params| service.validate_conversation_adapter(params),
        &[param!("manifest_path", "Adapter manifest path", ["manifestPath"])],
        None
    ),
    command!(
        "conversation.adapter.runtime-status",
        "conversation.adapter.runtime-status",
        "List detected system runtimes for conversation adapters",
        Read,
        Friendly,
        false,
        NoParams,
        ServiceAsync => |service, _params| service.list_conversation_adapter_runtime_statuses().await,
        &[],
        None
    ),
    command!(
        "conversation.adapter.register",
        "conversation.adapter.register",
        "Register a trusted conversation adapter script",
        HighRiskWrite,
        Friendly,
        true,
        crate::backend::infrastructure::conversations::ExternalAdapterRegisterParams,
        ServiceAsync => |service, params| service.register_conversation_adapter(params).await,
        &[
            param!("manifest_path", "Adapter manifest path", ["manifestPath"]),
            param!("dry_run", "Preview without persisting", ["dryRun"]),
            param!("yes", "Confirm trusting this adapter"),
        ],
        None
    ),
    command!(
        "conversation.adapter.unregister",
        "conversation.adapter.unregister",
        "Unregister an external conversation adapter",
        HighRiskWrite,
        Friendly,
        true,
        crate::backend::application::ConversationAdapterUnregisterParams,
        ServiceAsync => |service, params| service.unregister_conversation_adapter(params).await,
        &[
            param!("adapter_id", "Adapter identifier", ["adapterId"]),
            param!("dry_run", "Preview without unregistering", ["dryRun"]),
            param!("yes", "Confirm unregistering this adapter"),
        ],
        None
    ),
    command!(
        "conversation.adapter.try-run",
        "conversation.adapter.try-run",
        "Run a conversation adapter manifest once and validate its NDJSON output",
        HighRiskWrite,
        Friendly,
        false,
        crate::backend::infrastructure::conversations::ExternalAdapterTryRunParams,
        ServiceAsync => |service, params| service.try_run_conversation_adapter(params).await,
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
        "conversation.source.list",
        "conversation.source.list",
        "List conversation sources",
        Read,
        Friendly,
        false,
        NoParams,
        ServiceAsync => |service, _params| service.list_conversation_sources().await,
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
        ServiceAsync => |service, params| service.upsert_conversation_source(params).await,
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
        ServiceAsync => |service, params| service.upsert_conversation_source(params).await,
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
        ServiceAsync => |service, params| service.disable_conversation_source(params).await,
        &[
            param!("id", "Conversation source identifier"),
            param!("dry_run", "Preview without disabling", ["dryRun"]),
        ],
        Some("assetiweave-cli conversation source disable <source-id>")
    ),
    command!(
        "conversation.script.catalog",
        "conversation.script.catalog",
        "List downloadable conversation parser scripts from a catalog",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationScriptCatalogParams,
        ServiceAsync => |service, params| service.list_conversation_script_catalog(params).await,
        &[param!(
            "catalog_url",
            "Optional catalog JSON URL or local path",
            ["catalogUrl"]
        )],
        None
    ),
    command!(
        "conversation.script.install",
        "conversation.script.install",
        "Download and register a trusted conversation parser script",
        HighRiskWrite,
        Friendly,
        true,
        crate::backend::application::ConversationScriptInstallParams,
        ServiceAsync => |service, params| service.install_conversation_script(params).await,
        &[
            param!(
                "catalog_url",
                "Optional catalog JSON URL or local path",
                ["catalogUrl"]
            ),
            param!("item_id", "Catalog item identifier", ["itemId"]),
            param!("dry_run", "Preview install target without downloading", ["dryRun"]),
            param!("yes", "Confirm downloading and trusting this script"),
        ],
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
        "Create a system-runtime conversation adapter manifest scaffold",
        Write,
        App,
        false,
        crate::backend::infrastructure::conversations::ExternalAdapterScaffoldParams,
        Service => |service, params| service.scaffold_conversation_adapter(params),
        &[
            param!("directory", "Directory where scaffold files will be created"),
            param!("id", "Adapter identifier"),
            param!("name", "Adapter display name"),
            param!("runtime_type", "Adapter runtime: node, python, bash, or executable", ["runtimeType"]),
            param!("runtime_entry", "Adapter entry path relative to the adapter directory", ["runtimeEntry"]),
            param!("runtime_version", "Runtime version requirement such as >=20 or >=3.10", ["runtimeVersion"]),
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
        crate::backend::infrastructure::conversations::ExternalAdapterValidateParams,
        Service => |service, params| service.validate_conversation_adapter(params),
        &[param!("manifest_path", "Adapter manifest path", ["manifestPath"])],
        None
    ),
    command!(
        "list_conversation_adapter_runtime_statuses",
        "conversation.adapter.runtime-status",
        "List detected system runtimes for conversation adapters",
        Read,
        App,
        false,
        NoParams,
        ServiceAsync => |service, _params| service.list_conversation_adapter_runtime_statuses().await,
        &[],
        None
    ),
    command!(
        "register_conversation_adapter",
        "conversation.adapter.register",
        "Register a trusted conversation adapter script",
        HighRiskWrite,
        App,
        false,
        crate::backend::infrastructure::conversations::ExternalAdapterRegisterParams,
        ServiceAsync => |service, params| service.register_conversation_adapter(params).await,
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
        ServiceAsync => |service, params| service.unregister_conversation_adapter(params).await,
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
        crate::backend::infrastructure::conversations::ExternalAdapterTryRunParams,
        ServiceAsync => |service, params| service.try_run_conversation_adapter(params).await,
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
        ServiceAsync => |service, _params| service.list_conversation_sources().await,
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
        ServiceAsync => |service, params| service.upsert_conversation_source(params).await,
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
        ServiceAsync => |service, params| service.disable_conversation_source(params).await,
        &[
            param!("id", "Conversation source identifier"),
            param!("dry_run", "Preview without disabling", ["dryRun"]),
        ],
        None
    ),
    command!(
        "list_conversation_script_catalog",
        "conversation.script.catalog",
        "List downloadable conversation parser scripts from a catalog",
        Read,
        App,
        false,
        crate::backend::application::ConversationScriptCatalogParams,
        ServiceAsync => |service, params| service.list_conversation_script_catalog(params).await,
        &[param!(
            "catalog_url",
            "Optional catalog JSON URL or local path",
            ["catalogUrl"]
        )],
        None
    ),
    command!(
        "register_conversation_adapter_local",
        "conversation.adapter_package.register_local",
        "Register an external conversation adapter package without copying its files",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::ConversationAdapterLocalRegisterParams,
        ServiceAsync => |service, params| service.register_conversation_adapter_local(params).await,
        &[
            param!("package_dir", "Existing local package directory", ["packageDir"]),
            param!("origin", "Local package origin"),
            param!("source_url", "Optional source URL", ["sourceUrl"]),
            param!("git_ref", "Optional Git ref", ["gitRef"]),
            param!("git_commit", "Optional Git commit", ["gitCommit"]),
            param!("dry_run", "Validate and preview registration", ["dryRun"]),
            param!("yes", "Confirm trusting and registering the package"),
        ],
        None
    ),
    command!(
        "refresh_conversation_adapter_catalogs",
        "conversation.adapter_package.refresh_catalogs",
        "Refresh conversation adapter Catalog v2 caches",
        Write,
        App,
        false,
        crate::backend::application::ConversationAdapterCatalogRefreshParams,
        ServiceAsync => |service, params| service.refresh_conversation_adapter_catalogs(params).await,
        &[
            param!("catalog_url", "Optional Catalog v2 index URL or local path", ["catalogUrl"]),
            param!("force", "Ignore the 24 hour cache window"),
        ],
        None
    ),
    command!(
        "install_conversation_script",
        "conversation.script.install",
        "Download and register a trusted conversation parser script",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::ConversationScriptInstallParams,
        ServiceAsync => |service, params| service.install_conversation_script(params).await,
        &[
            param!(
                "catalog_url",
                "Optional catalog JSON URL or local path",
                ["catalogUrl"]
            ),
            param!("item_id", "Catalog item identifier", ["itemId"]),
            param!("dry_run", "Preview install target without downloading", ["dryRun"]),
            param!("yes", "Confirm downloading and trusting this script"),
        ],
        None
    ),
];
