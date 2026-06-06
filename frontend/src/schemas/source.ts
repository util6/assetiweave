import * as z from "zod";
import type { SourceInput } from "../types";
import {
  appKindSchema,
  assetKindSchema,
  sourceKindSchema,
  sourceOriginSchema,
  sourceScannerKindSchema,
} from "./domain";

const nonEmptyStringSchema = z.string().trim().min(1);
const globListSchema = z.array(nonEmptyStringSchema);

export const sourceInputSchema = z.strictObject({
  default_kind: assetKindSchema.nullable().default(null),
  enabled: z.boolean().default(true),
  exclude_globs: globListSchema.default([]),
  id: nonEmptyStringSchema.optional(),
  include_globs: globListSchema.default([]),
  kind: sourceKindSchema.default("local"),
  name: nonEmptyStringSchema,
  origin_app_kind: appKindSchema.nullable().default(null),
  priority: z.number().int().default(0),
  repo_root: z.string().trim().nullable().default(null),
  root_path: nonEmptyStringSchema,
  scan_root: z.string().trim().default(""),
  scanner_kind: sourceScannerKindSchema.default("mixed"),
  source_origin: sourceOriginSchema.default("local_folder"),
}) satisfies z.ZodType<SourceInput>;

export type ParsedSourceInput = z.output<typeof sourceInputSchema>;
