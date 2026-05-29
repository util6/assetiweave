import { describe, expect, it } from "vitest";
import { sourceInputSchema } from "./source";
import { validateWithSchema } from "./validation";

describe("source input schema", () => {
  it("applies defaults for optional source configuration fields", () => {
    const result = validateWithSchema(sourceInputSchema, {
      name: "  Local skills  ",
      root_path: "~/.codex/skills",
    });

    expect(result).toEqual({
      data: {
        default_kind: null,
        enabled: true,
        exclude_globs: [],
        include_globs: [],
        kind: "local",
        name: "Local skills",
        origin_app_kind: null,
        priority: 0,
        repo_root: null,
        root_path: "~/.codex/skills",
        scan_root: "",
        scanner_kind: "mixed",
        source_origin: "local_folder",
      },
      ok: true,
    });
  });

  it("rejects unknown source configuration keys", () => {
    const result = validateWithSchema(sourceInputSchema, {
      name: "Local skills",
      root_path: "~/.codex/skills",
      unexpected: true,
    });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errors.formErrors).toEqual([expect.stringContaining("Unrecognized key")]);
    }
  });

  it("rejects invalid source enum values", () => {
    const result = validateWithSchema(sourceInputSchema, {
      kind: "remote",
      name: "Local skills",
      root_path: "~/.codex/skills",
    });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errors.fieldErrors.kind).toEqual([expect.any(String)]);
    }
  });
});
