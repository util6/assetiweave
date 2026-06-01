import * as z from "zod";
import type {
  ApplySkillGroupExclusiveMountResult,
  ApplyAssetGroupMountResult,
  AssetGroup,
  AssetGroupDetail,
  AssetGroupInput,
  AssetGroupMemberOrigin,
  AssetGroupResolvedMember,
  AssetGroupRules,
  SkillGroupExclusiveMountInput,
  SkillGroupExclusiveMountItem,
  SkillGroupExclusiveMountPreview,
  SkillGroupExclusiveMountSkippedItem,
} from "../types";
import { assetKindSchema, deploymentStrategySchema, physicalMountStateSchema } from "./domain";

const nonEmptyStringSchema = z.string().trim().min(1);

export const assetGroupRulesSchema = z.strictObject({
  source_ids: z.array(nonEmptyStringSchema).default([]),
  relative_path_globs: z.array(nonEmptyStringSchema).default([]),
  name_contains: z.string().trim().nullable().default(null),
}) satisfies z.ZodType<AssetGroupRules>;

export const assetGroupSchema = z.strictObject({
  id: nonEmptyStringSchema,
  name: nonEmptyStringSchema,
  description: z.string().nullable().default(null),
  color: nonEmptyStringSchema,
  asset_kind: assetKindSchema,
  enabled: z.boolean(),
  sort_order: z.number().int(),
  rules: assetGroupRulesSchema,
  created_at: nonEmptyStringSchema,
  updated_at: nonEmptyStringSchema,
}) satisfies z.ZodType<AssetGroup>;

export const assetGroupMemberOriginSchema = z.enum([
  "manual",
  "rule",
  "manual_and_rule",
] as const satisfies readonly AssetGroupMemberOrigin[]);

export const assetGroupResolvedMemberSchema = z.strictObject({
  asset_id: nonEmptyStringSchema,
  origin: assetGroupMemberOriginSchema,
}) satisfies z.ZodType<AssetGroupResolvedMember>;

export const assetGroupDetailSchema = z.strictObject({
  group: assetGroupSchema,
  members: z.array(assetGroupResolvedMemberSchema),
  manual_asset_ids: z.array(nonEmptyStringSchema),
}) satisfies z.ZodType<AssetGroupDetail>;

export const assetGroupDetailListSchema = z.array(assetGroupDetailSchema);

export const assetGroupInputSchema = z.strictObject({
  id: nonEmptyStringSchema.optional(),
  name: nonEmptyStringSchema,
  description: z.string().nullable().optional(),
  color: z.string().trim().nullable().optional(),
  enabled: z.boolean().optional(),
  sort_order: z.number().int().optional(),
  rules: assetGroupRulesSchema.optional(),
}) satisfies z.ZodType<AssetGroupInput>;

const assetMountSchema = z.strictObject({
  asset_id: nonEmptyStringSchema,
  profile_id: nonEmptyStringSchema,
  enabled: z.boolean(),
  strategy: deploymentStrategySchema,
  created_at: nonEmptyStringSchema,
  updated_at: nonEmptyStringSchema,
});

const assetMountStatusSchema = z.strictObject({
  asset_id: nonEmptyStringSchema,
  profile_id: nonEmptyStringSchema,
  target_dir: z.string(),
  target_path: z.string(),
  state: physicalMountStateSchema,
  linked_source: z.string().nullable().optional(),
});

export const applyAssetGroupMountResultSchema = z.strictObject({
  group_id: nonEmptyStringSchema,
  profile_id: nonEmptyStringSchema,
  enabled: z.boolean(),
  requested_count: z.number().int().nonnegative(),
  updated_count: z.number().int().nonnegative(),
  error_count: z.number().int().nonnegative(),
  mounts: z.array(assetMountSchema),
  statuses: z.array(assetMountStatusSchema),
  errors: z.array(
    z.strictObject({
      asset_id: nonEmptyStringSchema,
      message: nonEmptyStringSchema,
    }),
  ),
}) satisfies z.ZodType<ApplyAssetGroupMountResult>;

export const skillGroupExclusiveMountInputSchema = z.strictObject({
  group_ids: z.array(nonEmptyStringSchema).min(1),
  profile_id: nonEmptyStringSchema,
  mount_selected: z.literal(true),
  dry_run: z.boolean(),
}) satisfies z.ZodType<SkillGroupExclusiveMountInput>;

const exclusiveMountItemSchema = z.strictObject({
  asset_id: nonEmptyStringSchema,
  name: nonEmptyStringSchema,
}) satisfies z.ZodType<SkillGroupExclusiveMountItem>;

const exclusiveMountSkippedItemSchema = exclusiveMountItemSchema.extend({
  reason: nonEmptyStringSchema,
}) satisfies z.ZodType<SkillGroupExclusiveMountSkippedItem>;

export const skillGroupExclusiveMountPreviewSchema = z.strictObject({
  profile_id: nonEmptyStringSchema,
  group_ids: z.array(nonEmptyStringSchema),
  selected_skill_ids: z.array(nonEmptyStringSchema),
  keep: z.array(exclusiveMountItemSchema),
  mount: z.array(exclusiveMountItemSchema),
  unmount: z.array(exclusiveMountItemSchema),
  skipped: z.array(exclusiveMountSkippedItemSchema),
  keep_count: z.number().int().nonnegative(),
  mount_count: z.number().int().nonnegative(),
  unmount_count: z.number().int().nonnegative(),
  skipped_count: z.number().int().nonnegative(),
}) satisfies z.ZodType<SkillGroupExclusiveMountPreview>;

export const applySkillGroupExclusiveMountResultSchema = skillGroupExclusiveMountPreviewSchema.extend({
  statuses: z.array(assetMountStatusSchema),
  errors: z.array(
    exclusiveMountItemSchema.extend({
      message: nonEmptyStringSchema,
    }),
  ),
}) satisfies z.ZodType<ApplySkillGroupExclusiveMountResult>;
