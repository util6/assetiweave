import { z } from "zod";
import type { SourceImportFormValues } from "../../utils/sourceImport";

export const sourceFormSchema = z.object({
  enabled: z.boolean(),
  excludeGlobsText: z.string(),
  includeGlobsText: z.string(),
  name: z.string(),
  priority: z.string().refine(
    (value) => {
      const trimmed = value.trim();
      if (trimmed === "") {
        return true;
      }
      const num = Number(trimmed);
      return Number.isInteger(num);
    },
    { message: "invalid" },
  ),
  rootPath: z
    .string()
    .refine((value) => value.trim().length > 0, { message: "required" }),
}) satisfies z.ZodType<SourceImportFormValues>;

export type SourceFormValues = z.infer<typeof sourceFormSchema>;
