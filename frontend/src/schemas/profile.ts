import * as z from "zod";
import type { ProfileSafety, TargetProfile, TargetProfileInput, TargetProfileRuleSet } from "../types";
import { appKindSchema, assetKindSchema, deploymentStrategySchema } from "./domain";

const nonEmptyStringSchema = z.string().trim().min(1);

export const targetProfileRuleSetSchema = z.strictObject({
  groups: z.array(nonEmptyStringSchema).default([]),
  kinds: z.array(assetKindSchema).default(["skill"]),
  path_patterns: z.array(nonEmptyStringSchema).default([]),
  sources: z.array(nonEmptyStringSchema).default([]),
  tags: z.array(nonEmptyStringSchema).default([]),
}) satisfies z.ZodType<TargetProfileRuleSet>;

export const profileSafetySchema = z.strictObject({
  allow_overwrite: z.boolean().default(false),
  allow_remove: z.boolean().default(false),
}) satisfies z.ZodType<ProfileSafety>;

export const targetProfileInputSchema = z.strictObject({
  app_kind: appKindSchema.default("custom"),
  deployment_strategy: deploymentStrategySchema.default("symlink_to_source"),
  enabled: z.boolean().default(true),
  exclude: targetProfileRuleSetSchema.default({
    groups: [],
    kinds: ["unclassified"],
    path_patterns: [],
    sources: [],
    tags: [],
  }),
  id: nonEmptyStringSchema.optional(),
  include: targetProfileRuleSetSchema.default({
    groups: [],
    kinds: ["skill"],
    path_patterns: [],
    sources: [],
    tags: [],
  }),
  name: nonEmptyStringSchema,
  safety: profileSafetySchema.default({ allow_overwrite: false, allow_remove: false }),
  supported_kinds: z.array(assetKindSchema).default(["skill"]),
  target_paths: z.array(nonEmptyStringSchema).min(1),
}) satisfies z.ZodType<TargetProfileInput>;

export const targetProfileSchema = z.strictObject({
  app_kind: appKindSchema,
  deployment_strategy: deploymentStrategySchema,
  enabled: z.boolean(),
  exclude: targetProfileRuleSetSchema,
  id: nonEmptyStringSchema,
  include: targetProfileRuleSetSchema,
  name: nonEmptyStringSchema,
  safety: profileSafetySchema,
  supported_kinds: z.array(assetKindSchema),
  target_paths: z.array(nonEmptyStringSchema).min(1),
}) satisfies z.ZodType<TargetProfile>;

export const targetProfileListSchema = z.array(targetProfileSchema);
