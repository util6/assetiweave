import * as z from "zod";
import type {
  Asset,
  SkillAcquireResult,
  SkillRemoteSource,
  SkillSearchCandidate,
  SkillSearchResult,
} from "../types";
import { assetFormatSchema, assetKindSchema } from "./domain";

const nonEmptyStringSchema = z.string().trim().min(1);

export const skillSearchCandidateSchema = z.strictObject({
  name: nonEmptyStringSchema,
  description: z.string().nullable().optional(),
  match_reason: z.string().nullable().optional(),
  url: nonEmptyStringSchema,
  path: z.string().nullable().optional(),
  clone_url: z.string().nullable().optional(),
  default_branch: z.string().nullable().optional(),
  stars: z.number().int().nonnegative().nullable().optional(),
  acquire_command: nonEmptyStringSchema,
}) satisfies z.ZodType<SkillSearchCandidate>;

export const skillSearchResultSchema = z.strictObject({
  query: nonEmptyStringSchema,
  provider: nonEmptyStringSchema,
  candidates: z.array(skillSearchCandidateSchema),
  warnings: z.array(z.string()).optional().default([]),
}) satisfies z.ZodType<SkillSearchResult>;

export const skillRemoteSourceSchema = z.strictObject({
  asset_id: nonEmptyStringSchema,
  provider: nonEmptyStringSchema,
  source_url: nonEmptyStringSchema,
  repo_url: nonEmptyStringSchema,
  branch: nonEmptyStringSchema,
  path: z.string().nullable().optional(),
  acquired_at: nonEmptyStringSchema,
  acquired_tree_sha: z.string().nullable().optional(),
  local_content_hash: z.string().nullable().optional(),
  last_checked_at: z.string().nullable().optional(),
  latest_tree_sha: z.string().nullable().optional(),
  status: z.enum(["unknown", "current", "changed", "error"]),
  message: z.string().nullable().optional(),
}) satisfies z.ZodType<SkillRemoteSource>;

const assetSchema = z.strictObject({
  id: nonEmptyStringSchema,
  source_id: nonEmptyStringSchema,
  name: nonEmptyStringSchema,
  kind: assetKindSchema,
  format: assetFormatSchema,
  relative_path: z.string(),
  absolute_path: z.string(),
  entry_file: z.string().nullable().optional(),
  description: z.string().nullable().optional(),
  content_hash: z.string().nullable().optional(),
  discovered_at: nonEmptyStringSchema,
  updated_at: nonEmptyStringSchema,
}) satisfies z.ZodType<Asset>;

export const skillAcquireResultSchema = z.strictObject({
  dry_run: z.boolean(),
  provider: nonEmptyStringSchema,
  url: nonEmptyStringSchema,
  repo_url: nonEmptyStringSchema,
  branch: z.string().nullable().optional(),
  path: z.string().nullable().optional(),
  name: nonEmptyStringSchema,
  staging_path: nonEmptyStringSchema,
  skill_path: nonEmptyStringSchema,
  security_notice: z.string().nullable().optional(),
  import: z
    .strictObject({
      dry_run: z.boolean(),
      asset: assetSchema.optional(),
    })
    .optional(),
  remote_source: skillRemoteSourceSchema.optional(),
}) satisfies z.ZodType<SkillAcquireResult>;
