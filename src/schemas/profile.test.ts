import { describe, expect, it } from "vitest";
import { targetProfileInputSchema, targetProfileSchema } from "./profile";
import { validateWithSchema } from "./validation";

describe("target profile input schema", () => {
  it("applies skill mount defaults for imported apps", () => {
    const result = validateWithSchema(targetProfileInputSchema, {
      name: "  Team App  ",
      target_paths: ["  ~/team-app/skills  "],
    });

    expect(result).toEqual({
      data: {
        app_kind: "custom",
        deployment_strategy: "symlink_to_source",
        enabled: true,
        exclude: { groups: [], kinds: ["unclassified"], path_patterns: [], sources: [], tags: [] },
        include: { groups: [], kinds: ["skill"], path_patterns: [], sources: [], tags: [] },
        name: "Team App",
        safety: { allow_overwrite: false, allow_remove: false },
        supported_kinds: ["skill"],
        target_paths: ["~/team-app/skills"],
      },
      ok: true,
    });
  });

  it("rejects missing target directories", () => {
    const result = validateWithSchema(targetProfileInputSchema, {
      name: "Team App",
      target_paths: [],
    });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errors.fieldErrors.target_paths).toEqual([expect.any(String)]);
    }
  });

  it("normalizes legacy OpenCode app kind names from persisted profiles", () => {
    const result = validateWithSchema(targetProfileSchema, {
      app_kind: "open_code",
      deployment_strategy: "symlink_to_source",
      enabled: true,
      exclude: { groups: [], kinds: ["unclassified"], path_patterns: [], sources: [], tags: [] },
      id: "opencode",
      include: { groups: [], kinds: ["skill"], path_patterns: [], sources: [], tags: [] },
      name: "OpenCode",
      safety: { allow_overwrite: false, allow_remove: false },
      supported_kinds: ["skill"],
      target_paths: ["~/.config/opencode/skills"],
    });

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.data.app_kind).toBe("opencode");
    }
  });
});
