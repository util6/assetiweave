//! Engine 命令注册表：Conversations / Packages

use super::super::dispatch::*;
use super::super::types::*;
use crate::backend::application::AppService;
use crate::{command, param};
use serde_json::{json, Value};

pub(super) const COMMANDS: &[CommandSpec] = &[
    command!(
        "conversation.adapter_package.register_local",
        "conversation.adapter_package.register_local",
        "Register an external conversation adapter package without copying its files",
        HighRiskWrite,
        Friendly,
        true,
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
        "conversation.adapter_package.upgrade_workspace",
        "conversation.adapter_package.upgrade_workspace",
        "Validate editable conversation adapter workspaces and promote immutable runtime copies",
        HighRiskWrite,
        Friendly,
        true,
        crate::backend::application::ConversationAdapterWorkspaceUpgradeParams,
        ServiceAsync => |service, params| service.upgrade_conversation_adapter_workspace(params).await,
        &[
            param!("package_dir", "Optional adapter workspace directory", ["packageDir"]),
            param!("developer", "Use builtin-assets/adapters from the current repository"),
            param!("dry_run", "Validate and preview promotion", ["dryRun"]),
        ],
        Some("assetiweave-cli conversation adapter upgrade [adapter-directory]")
    ),
    command!(
        "conversation.adapter_package.inspect",
        "conversation.adapter_package.inspect",
        "Inspect an active conversation adapter package or adapter runtime",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationAdapterPackageInspectParams,
        ServiceAsync => |service, params| service.inspect_conversation_adapter_package(params).await,
        &[
            param!("package_id", "Optional package identifier", ["packageId"]),
            param!("adapter_id", "Optional adapter identifier", ["adapterId"]),
        ],
        Some("assetiweave-cli conversation adapter inspect <package-or-adapter-id>")
    ),
    command!(
        "conversation.adapter_package.prepare_change",
        "conversation.adapter_package.prepare_change",
        "Preview a conversation adapter package lifecycle change",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationAdapterPackageChangeParams,
        ServiceAsync => |service, params| service.prepare_conversation_adapter_package_change(params).await,
        &[
            param!("action", "Lifecycle action to preview"),
            param!("package_id", "Optional package identifier", ["packageId"]),
            param!("adapter_id", "Optional adapter identifier", ["adapterId"]),
        ],
        None
    ),
    command!(
        "conversation.adapter_package.catalog",
        "conversation.adapter_package.catalog",
        "List downloadable conversation adapter packages from a catalog",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationAdapterPackageCatalogParams,
        ServiceAsync => |service, params| service.list_conversation_adapter_packages(params).await,
        &[param!(
            "catalog_url",
            "Optional catalog JSON URL or local path",
            ["catalogUrl"]
        )],
        Some("assetiweave-cli conversation adapter list")
    ),
    command!(
        "conversation.adapter_package.releases",
        "conversation.adapter_package.releases",
        "List version history and changelog for a conversation adapter package",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationAdapterPackageReleaseListParams,
        ServiceAsync => |service, params| service.list_conversation_adapter_package_releases(params).await,
        &[
            param!("catalog_url", "Optional Catalog v2 index URL or local path", ["catalogUrl"]),
            param!("package_id", "Package identifier", ["packageId"]),
            param!("refresh", "Refresh Catalog v2 before listing"),
        ],
        None
    ),
    command!(
        "conversation.adapter_package.installed_versions",
        "conversation.adapter_package.installed_versions",
        "List locally installed versions for a conversation adapter package",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationAdapterPackageVersionChangeParams,
        ServiceAsync => |service, params| service.list_installed_conversation_adapter_package_versions(params).await,
        &[param!("package_id", "Installed package identifier", ["packageId"])],
        None
    ),
    command!(
        "conversation.adapter_package.switch_version",
        "conversation.adapter_package.switch_version",
        "Activate an installed conversation adapter package version",
        HighRiskWrite,
        Friendly,
        true,
        crate::backend::application::ConversationAdapterPackageVersionChangeParams,
        ServiceAsync => |service, params| service.switch_conversation_adapter_package_version(params).await,
        &[param!("package_id", "Installed package identifier", ["packageId"]), param!("version", "Installed version"), param!("dry_run", "Preview activation", ["dryRun"]), param!("yes", "Confirm activation")],
        None
    ),
    command!(
        "conversation.adapter_package.rollback",
        "conversation.adapter_package.rollback",
        "Rollback to the most recently installed inactive package version",
        HighRiskWrite,
        Friendly,
        true,
        crate::backend::application::ConversationAdapterPackageVersionChangeParams,
        ServiceAsync => |service, params| service.rollback_conversation_adapter_package_version(params).await,
        &[param!("package_id", "Installed package identifier", ["packageId"]), param!("dry_run", "Preview rollback", ["dryRun"]), param!("yes", "Confirm rollback")],
        None
    ),
    command!(
        "conversation.adapter_package.delete_version",
        "conversation.adapter_package.delete_version",
        "Delete one inactive managed package version",
        HighRiskWrite,
        Friendly,
        true,
        crate::backend::application::ConversationAdapterPackageVersionChangeParams,
        ServiceAsync => |service, params| service.delete_conversation_adapter_package_version(params).await,
        &[param!("package_id", "Installed package identifier", ["packageId"]), param!("version", "Inactive installed version"), param!("dry_run", "Preview deletion", ["dryRun"]), param!("yes", "Confirm deletion")],
        None
    ),
    command!(
        "conversation.adapter_package.refresh_catalogs",
        "conversation.adapter_package.refresh_catalogs",
        "Refresh conversation adapter Catalog v2 caches",
        Write,
        Friendly,
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
        "conversation.adapter_package.check_updates",
        "conversation.adapter_package.check_updates",
        "Check installed conversation adapter packages for compatible updates",
        Read,
        Friendly,
        false,
        crate::backend::application::ConversationAdapterPackageUpdateCheckParams,
        ServiceAsync => |service, params| service.check_conversation_adapter_package_updates(params).await,
        &[
            param!("catalog_url", "Optional Catalog v2 index URL or local path", ["catalogUrl"]),
            param!("force", "Force a remote Catalog v2 refresh"),
        ],
        None
    ),
    command!(
        "conversation.adapter_package.set_update_policy",
        "conversation.adapter_package.set_update_policy",
        "Set the update-follow policy for an installed conversation adapter package",
        Write,
        Friendly,
        false,
        crate::backend::application::ConversationAdapterPackageUpdatePolicyParams,
        ServiceAsync => |service, params| service.set_conversation_adapter_package_update_policy(params).await,
        &[param!("package_id", "Installed package identifier", ["packageId"]), param!("update_policy", "manual, follow_stable, follow_beta, or pin_exact", ["updatePolicy"])],
        None
    ),
    command!(
        "conversation.adapter_package.install",
        "conversation.adapter_package.install",
        "Download and register a trusted conversation adapter package",
        HighRiskWrite,
        Friendly,
        true,
        crate::backend::application::ConversationAdapterPackageInstallParams,
        ServiceAsync => |service, params| service.install_conversation_adapter_package(params).await,
        &[
            param!(
                "catalog_url",
                "Optional catalog JSON URL or local path",
                ["catalogUrl"]
            ),
            param!("package_id", "Catalog package identifier", ["packageId", "itemId"]),
            param!("version", "Optional exact compatible SemVer release"),
            param!("dry_run", "Preview install target without downloading", ["dryRun"]),
            param!("yes", "Confirm downloading and trusting this package"),
        ],
        None
    ),
    command!(
        "conversation.adapter_package.update",
        "conversation.adapter_package.update",
        "Update a trusted conversation adapter package",
        HighRiskWrite,
        Friendly,
        true,
        crate::backend::application::ConversationAdapterPackageInstallParams,
        ServiceAsync => |service, params| service.update_conversation_adapter_package(params).await,
        &[
            param!(
                "catalog_url",
                "Optional catalog JSON URL or local path",
                ["catalogUrl"]
            ),
            param!("package_id", "Catalog package identifier", ["packageId", "itemId"]),
            param!("version", "Optional exact compatible SemVer release"),
            param!("dry_run", "Preview update target without downloading", ["dryRun"]),
            param!("yes", "Confirm downloading and trusting this package"),
        ],
        None
    ),
    command!(
        "conversation.adapter_package.uninstall",
        "conversation.adapter_package.uninstall",
        "Uninstall a conversation adapter runtime while retaining managed package files and conversation records",
        HighRiskWrite,
        Friendly,
        true,
        crate::backend::application::ConversationAdapterPackageUninstallParams,
        ServiceAsync => |service, params| service.uninstall_conversation_adapter_package(params).await,
        &[
            param!("package_id", "Installed package identifier", ["packageId"]),
            param!("dry_run", "Preview uninstall without changing state", ["dryRun"]),
            param!("yes", "Confirm unregistering this package"),
        ],
        None
    ),
    command!(
        "inspect_conversation_adapter_package",
        "conversation.adapter_package.inspect",
        "Inspect an active conversation adapter package or adapter runtime",
        Read,
        App,
        false,
        crate::backend::application::ConversationAdapterPackageInspectParams,
        ServiceAsync => |service, params| service.inspect_conversation_adapter_package(params).await,
        &[
            param!("package_id", "Optional package identifier", ["packageId"]),
            param!("adapter_id", "Optional adapter identifier", ["adapterId"]),
        ],
        None
    ),
    command!(
        "prepare_conversation_adapter_package_change",
        "conversation.adapter_package.prepare_change",
        "Preview a conversation adapter package lifecycle change",
        Read,
        App,
        false,
        crate::backend::application::ConversationAdapterPackageChangeParams,
        ServiceAsync => |service, params| service.prepare_conversation_adapter_package_change(params).await,
        &[
            param!("action", "Lifecycle action to preview"),
            param!("package_id", "Optional package identifier", ["packageId"]),
            param!("adapter_id", "Optional adapter identifier", ["adapterId"]),
        ],
        None
    ),
    command!(
        "list_conversation_adapter_packages",
        "conversation.adapter_package.catalog",
        "List downloadable conversation adapter packages from a catalog",
        Read,
        App,
        false,
        crate::backend::application::ConversationAdapterPackageCatalogParams,
        ServiceAsync => |service, params| service.list_conversation_adapter_packages(params).await,
        &[param!(
            "catalog_url",
            "Optional catalog JSON URL or local path",
            ["catalogUrl"]
        )],
        None
    ),
    command!(
        "list_conversation_adapter_package_releases",
        "conversation.adapter_package.releases",
        "List version history and changelog for a conversation adapter package",
        Read,
        App,
        false,
        crate::backend::application::ConversationAdapterPackageReleaseListParams,
        ServiceAsync => |service, params| service.list_conversation_adapter_package_releases(params).await,
        &[
            param!("catalog_url", "Optional Catalog v2 index URL or local path", ["catalogUrl"]),
            param!("package_id", "Package identifier", ["packageId"]),
            param!("refresh", "Refresh Catalog v2 before listing"),
        ],
        None
    ),
    command!(
        "list_installed_conversation_adapter_package_versions",
        "conversation.adapter_package.installed_versions",
        "List locally installed versions for a conversation adapter package",
        Read,
        App,
        false,
        crate::backend::application::ConversationAdapterPackageVersionChangeParams,
        ServiceAsync => |service, params| service.list_installed_conversation_adapter_package_versions(params).await,
        &[param!("package_id", "Installed package identifier", ["packageId"])],
        None
    ),
    command!(
        "switch_conversation_adapter_package_version",
        "conversation.adapter_package.switch_version",
        "Activate an installed conversation adapter package version",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::ConversationAdapterPackageVersionChangeParams,
        ServiceAsync => |service, params| service.switch_conversation_adapter_package_version(params).await,
        &[param!("package_id", "Installed package identifier", ["packageId"]), param!("version", "Installed version"), param!("dry_run", "Preview activation", ["dryRun"]), param!("yes", "Confirm activation")],
        None
    ),
    command!(
        "rollback_conversation_adapter_package_version",
        "conversation.adapter_package.rollback",
        "Rollback to the most recently installed inactive package version",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::ConversationAdapterPackageVersionChangeParams,
        ServiceAsync => |service, params| service.rollback_conversation_adapter_package_version(params).await,
        &[param!("package_id", "Installed package identifier", ["packageId"]), param!("dry_run", "Preview rollback", ["dryRun"]), param!("yes", "Confirm rollback")],
        None
    ),
    command!(
        "delete_conversation_adapter_package_version",
        "conversation.adapter_package.delete_version",
        "Delete one inactive managed package version",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::ConversationAdapterPackageVersionChangeParams,
        ServiceAsync => |service, params| service.delete_conversation_adapter_package_version(params).await,
        &[param!("package_id", "Installed package identifier", ["packageId"]), param!("version", "Inactive installed version"), param!("dry_run", "Preview deletion", ["dryRun"]), param!("yes", "Confirm deletion")],
        None
    ),
    command!(
        "check_conversation_adapter_package_updates",
        "conversation.adapter_package.check_updates",
        "Check installed conversation adapter packages for compatible updates",
        Read,
        App,
        false,
        crate::backend::application::ConversationAdapterPackageUpdateCheckParams,
        ServiceAsync => |service, params| service.check_conversation_adapter_package_updates(params).await,
        &[
            param!("catalog_url", "Optional Catalog v2 index URL or local path", ["catalogUrl"]),
            param!("force", "Force a remote Catalog v2 refresh"),
        ],
        None
    ),
    command!(
        "set_conversation_adapter_package_update_policy",
        "conversation.adapter_package.set_update_policy",
        "Set the update-follow policy for an installed conversation adapter package",
        Write,
        App,
        false,
        crate::backend::application::ConversationAdapterPackageUpdatePolicyParams,
        ServiceAsync => |service, params| service.set_conversation_adapter_package_update_policy(params).await,
        &[param!("package_id", "Installed package identifier", ["packageId"]), param!("update_policy", "manual, follow_stable, follow_beta, or pin_exact", ["updatePolicy"])],
        None
    ),
    command!(
        "install_conversation_adapter_package",
        "conversation.adapter_package.install",
        "Download and register a trusted conversation adapter package",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::ConversationAdapterPackageInstallParams,
        ServiceAsync => |service, params| service.install_conversation_adapter_package(params).await,
        &[
            param!(
                "catalog_url",
                "Optional catalog JSON URL or local path",
                ["catalogUrl"]
            ),
            param!("package_id", "Catalog package identifier", ["packageId", "itemId"]),
            param!("version", "Optional exact compatible SemVer release"),
            param!("dry_run", "Preview install target without downloading", ["dryRun"]),
            param!("yes", "Confirm downloading and trusting this package"),
        ],
        None
    ),
    command!(
        "update_conversation_adapter_package",
        "conversation.adapter_package.update",
        "Update a trusted conversation adapter package",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::ConversationAdapterPackageInstallParams,
        ServiceAsync => |service, params| service.update_conversation_adapter_package(params).await,
        &[
            param!(
                "catalog_url",
                "Optional catalog JSON URL or local path",
                ["catalogUrl"]
            ),
            param!("package_id", "Catalog package identifier", ["packageId", "itemId"]),
            param!("version", "Optional exact compatible SemVer release"),
            param!("dry_run", "Preview update target without downloading", ["dryRun"]),
            param!("yes", "Confirm downloading and trusting this package"),
        ],
        None
    ),
    command!(
        "uninstall_conversation_adapter_package",
        "conversation.adapter_package.uninstall",
        "Uninstall a conversation adapter runtime while retaining managed package files and conversation records",
        HighRiskWrite,
        App,
        false,
        crate::backend::application::ConversationAdapterPackageUninstallParams,
        ServiceAsync => |service, params| service.uninstall_conversation_adapter_package(params).await,
        &[
            param!("package_id", "Installed package identifier", ["packageId"]),
            param!("dry_run", "Preview uninstall without changing state", ["dryRun"]),
            param!("yes", "Confirm unregistering this package"),
        ],
        None
    ),
];
