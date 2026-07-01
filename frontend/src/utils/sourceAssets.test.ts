import { describe, expect, it } from "vitest";
import type { Asset, Source } from "../types";
import { groupSourceDisplayAssets } from "./sourceAssets";

describe("groupSourceDisplayAssets", () => {
  it("shows hidden backed-up copies under the backup library source", () => {
    const grouped = groupSourceDisplayAssets(
      [
        asset("source-canvas", "source-a", {
          name: "canvas-design",
          backup_status: {
            state: "backed_up",
            backup_path: "/Users/util6/.assetiweave/library/skills/backed-up/source-a/canvas-design",
            hidden_asset_ids: ["backup-canvas"],
          },
        }),
      ],
      [source("source-a", "Source A", "local_folder"), source("assetiweave-library-skills", "Backup Library", "assetiweave_library")],
    );

    expect(grouped.get("source-a")?.map((candidate) => candidate.id)).toEqual(["source-canvas"]);
    expect(grouped.get("assetiweave-library-skills")).toMatchObject([
      {
        id: "backup-canvas",
        source_id: "assetiweave-library-skills",
        name: "canvas-design",
        absolute_path: "/Users/util6/.assetiweave/library/skills/backed-up/source-a/canvas-design",
        backup_status: {
          state: "backed_up",
          hidden_asset_ids: [],
        },
      },
    ]);
  });

  it("does not duplicate backup assets that are already visible", () => {
    const grouped = groupSourceDisplayAssets(
      [
        asset("backup-canvas", "assetiweave-library-skills", { name: "canvas-design" }),
        asset("source-canvas", "source-a", {
          name: "canvas-design",
          backup_status: {
            state: "backed_up",
            backup_path: "/Users/util6/.assetiweave/library/skills/backed-up/source-a/canvas-design",
            hidden_asset_ids: ["backup-canvas"],
          },
        }),
      ],
      [source("source-a", "Source A", "local_folder"), source("assetiweave-library-skills", "Backup Library", "assetiweave_library")],
    );

    expect(grouped.get("assetiweave-library-skills")?.map((candidate) => candidate.id)).toEqual(["backup-canvas"]);
  });
});

function asset(id: string, sourceId: string, overrides: Partial<Asset> = {}): Asset {
  return {
    id,
    source_id: sourceId,
    name: id,
    kind: "skill",
    format: "directory",
    relative_path: id,
    absolute_path: `/tmp/${id}`,
    entry_file: null,
    description: null,
    content_hash: null,
    discovered_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

function source(id: string, name: string, sourceOrigin: Source["source_origin"]): Source {
  return {
    id,
    name,
    kind: "local",
    root_path: id === "assetiweave-library-skills" ? "/Users/util6/.assetiweave/library/skills" : `/tmp/${id}`,
    scanner_kind: "skill",
    source_origin: sourceOrigin,
    repo_root: null,
    scan_root: "",
    origin_app_kind: null,
    include_globs: ["**/SKILL.md"],
    exclude_globs: [],
    default_kind: "skill",
    enabled: true,
    priority: 0,
    last_scanned_at: null,
    last_scan_status: null,
  };
}
