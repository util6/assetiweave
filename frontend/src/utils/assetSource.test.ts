import { describe, expect, it } from "vitest";
import type { Asset, Source } from "../types";
import { assetSourceHref, assetSourceLabel } from "./assetSource";

describe("assetSourceLabel", () => {
  it("prefers the precise repository remote over the source id", () => {
    expect(
      assetSourceLabel(
        {
          ...asset,
          repository: {
            root_path: "/Users/util6/fork-code/skills-fork/claude-skills",
            remote_url: "https://github.com/anthropics/skills.git",
          },
        },
        source,
      ),
    ).toBe("https://github.com/anthropics/skills.git");
  });

  it("falls back to repository root and then source name", () => {
    expect(
      assetSourceLabel(
        {
          ...asset,
          repository: {
            root_path: "/Users/util6/code-space/util6-agents",
            remote_url: null,
          },
        },
        source,
      ),
    ).toBe("/Users/util6/code-space/util6-agents");
    expect(assetSourceLabel(asset, source)).toBe("Agent Skills");
  });

  it("uses the source root instead of a source name that repeats its id", () => {
    expect(
      assetSourceLabel(asset, {
        ...source,
        name: source.id,
      }),
    ).toBe("/tmp");
  });

  it("returns the repository browser url when one is available", () => {
    expect(
      assetSourceHref({
        ...asset,
        repository: {
          root_path: "/Users/util6/code-space/util6-agents",
          remote_url: "https://github.com/util6/util6-agents.git",
          web_url: "https://github.com/util6/util6-agents/tree/main/skills/zh-cn/office-utils",
        },
      }),
    ).toBe("https://github.com/util6/util6-agents/tree/main/skills/zh-cn/office-utils");
    expect(assetSourceHref(asset)).toBeUndefined();
  });
});

const asset: Asset = {
  id: "hash-like-asset-id",
  source_id: "hash-like-source-id",
  name: "demo",
  kind: "skill",
  format: "directory",
  relative_path: "demo",
  absolute_path: "/tmp/demo",
  entry_file: null,
  description: null,
  content_hash: null,
  discovered_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};

const source: Source = {
  id: "hash-like-source-id",
  name: "Agent Skills",
  kind: "local",
  root_path: "/tmp",
  scanner_kind: "skill",
  source_origin: "local_folder",
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
