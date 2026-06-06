import { describe, expect, it } from "vitest";
import { parseSchemaOrThrow } from "./validation";
import {
  applySkillGroupExclusiveMountResultSchema,
  assetGroupDetailSchema,
  assetGroupInputSchema,
  skillGroupExclusiveMountInputSchema,
  skillGroupExclusiveMountPreviewSchema,
} from "./group";

describe("asset group schemas", () => {
  it("parses a skill group detail contract", () => {
    const detail = parseSchemaOrThrow(
      assetGroupDetailSchema,
      {
        group: {
          id: "frontend",
          name: "Frontend",
          description: null,
          color: "#10b981",
          asset_kind: "skill",
          enabled: true,
          sort_order: 0,
          rules: {
            source_ids: ["source-a"],
            relative_path_globs: ["frontend/**"],
            name_contains: "ui",
          },
          created_at: "2026-01-01T00:00:00Z",
          updated_at: "2026-01-01T00:00:00Z",
        },
        members: [{ asset_id: "asset-a", origin: "manual_and_rule" }],
        manual_asset_ids: ["asset-a"],
      },
      "Invalid group",
    );

    expect(detail.group.rules.source_ids).toEqual(["source-a"]);
    expect(detail.members[0].origin).toBe("manual_and_rule");
  });

  it("applies defaults for group rules in input", () => {
    const input = parseSchemaOrThrow(assetGroupInputSchema, { name: "Frontend" }, "Invalid input");

    expect(input.rules).toBeUndefined();
    expect(input.name).toBe("Frontend");
  });

  it("parses exclusive group mount preview and apply contracts", () => {
    const input = parseSchemaOrThrow(
      skillGroupExclusiveMountInputSchema,
      {
        group_ids: ["frontend", "automation"],
        profile_id: "codex",
        mount_selected: true,
        dry_run: true,
      },
      "Invalid input",
    );
    expect(input.group_ids).toEqual(["frontend", "automation"]);

    const preview = parseSchemaOrThrow(
      skillGroupExclusiveMountPreviewSchema,
      {
        profile_id: "codex",
        group_ids: ["frontend", "automation"],
        selected_skill_ids: ["skill-a", "skill-b"],
        keep: [{ asset_id: "skill-a", name: "skill-a" }],
        mount: [{ asset_id: "skill-b", name: "skill-b" }],
        unmount: [{ asset_id: "skill-c", name: "skill-c" }],
        skipped: [{ asset_id: "skill-d", name: "skill-d", reason: "risk" }],
        keep_count: 1,
        mount_count: 1,
        unmount_count: 1,
        skipped_count: 1,
      },
      "Invalid preview",
    );
    expect(preview.selected_skill_ids).toEqual(["skill-a", "skill-b"]);

    const result = parseSchemaOrThrow(
      applySkillGroupExclusiveMountResultSchema,
      {
        ...preview,
        statuses: [
          {
            asset_id: "skill-a",
            profile_id: "codex",
            target_dir: "/target",
            target_path: "/target/skill-a",
            state: "mounted",
            linked_source: "/source/skill-a",
          },
        ],
        errors: [{ asset_id: "skill-e", name: "skill-e", message: "failed" }],
      },
      "Invalid result",
    );
    expect(result.errors[0].message).toBe("failed");
  });
});
