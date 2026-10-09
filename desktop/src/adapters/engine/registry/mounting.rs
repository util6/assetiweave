//! Engine 命令注册表：Mounting 领域

use super::dispatch::*;
use super::types::*;
use crate::adapters::engine::protocol;
use crate::backend::application::AppService;
use crate::{command, param};
use serde_json::{json, Value};

pub(super) const COMMANDS: &[CommandSpec] = &[
    command!(
        "profile.list",
        "profile.list",
        "List target profiles",
        Read,
        Friendly,
        false,
        NoParams,
        ServiceAsync => |service, _params| service.list_profiles().await,
        &[],
        Some("assetiweave-cli profile list")
    ),
    command!(
        "list_target_profile_descriptors",
        "target.catalog.list",
        "List target provider descriptors",
        Read,
        App,
        false,
        NoParams,
        Service => |service, _params| service.list_target_profile_descriptors(),
        &[],
        None,
        since: "0.6.1", deprecated: false
    ),
    command!(
        "refresh_target_profile_descriptors",
        "target.catalog.refresh",
        "Reload target provider descriptors from the app-owned override directory",
        Write,
        App,
        false,
        NoParams,
        ServiceAsync => |service, _params| service.refresh_target_profile_descriptors().await,
        &[],
        None,
        since: "0.6.1", deprecated: false
    ),
    command!(
        "list_profiles",
        "profile.list",
        "List target profiles",
        Read,
        App,
        false,
        NoParams,
        ServiceAsync => |service, _params| service.list_profiles().await,
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
        ServiceAsync => |service, params| service.create_profile(params.input).await,
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
        ServiceAsync => |service, params| service.update_profile(params.profile).await,
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
        ServiceAsync => |service, params| service.delete_profile(params.id).await,
        &[param!("id", "Target profile identifier")],
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
        ServiceAsync => |service, params| service.list_asset_mounts(params.asset_id.as_deref()).await,
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
        ServiceAsync => |service, params| service.list_asset_mount_statuses(params.asset_id.as_deref()).await,
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
        ServiceAsync => |service, params| service.refresh_asset_mount_statuses(params.asset_id.as_deref()).await,
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
        ServiceAsync => |service, _params| service.list_skill_groups().await,
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
        ServiceAsync => |service, params| service.create_skill_group(params.input).await,
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
        ServiceAsync => |service, params| service.update_skill_group(params.group).await,
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
        ServiceAsync => |service, params| service.delete_skill_group(params.group_id).await,
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
        ServiceAsync => |service, params| service.set_skill_group_manual_members(params.group_id, params.asset_ids).await,
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
        ServiceAsync => |service, params| service.apply_skill_group_mount(&params.group_id, &params.profile_id, params.enabled).await,
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
        ServiceAsync => |service, params| service.preview_skill_group_exclusive_mount(params.input).await,
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
        ServiceAsync => |service, params| service.apply_skill_group_exclusive_mount(params.input).await,
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
        ServiceAsync => |service, params| service.toggle_asset_mount(&params.asset_id, &params.profile_id).await,
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
        ServiceAsync => |service, params| service.mount_asset_by_id(&params.asset_id, &params.profile_id).await,
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
        ServiceAsync => |service, params| service.unmount_asset_by_id(&params.asset_id, &params.profile_id).await,
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
        ServiceAsync => |service, params| service.set_asset_mount(&params.asset_id, &params.profile_id, params.enabled, params.strategy).await,
        &[
            param!("asset_id", "Asset identifier", ["assetId"]),
            param!("profile_id", "Target profile identifier", ["profileId"]),
            param!("enabled", "Requested mount state"),
            param!("strategy", "Optional deployment strategy"),
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
        ServiceAsync => |service, params| service.create_plan(params.profile_id.as_deref()).await,
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
        ServiceAsync => |service, params| service.execute_plan(params.plan, params.action_ids).await,
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
];
