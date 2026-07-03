import { describe, expect, it } from "vitest";
import type { Asset } from "../../types";
import { filterAssets } from "./useAssetFilter";

const baseAsset: Asset = {
  absolute_path: "/Users/util6/.assetiweave/assets/base.md",
  discovered_at: "2026-01-01T00:00:00Z",
  format: "markdown",
  id: "base",
  kind: "skill",
  name: "Base",
  relative_path: "base.md",
  source_id: "source-a",
  updated_at: "2026-01-01T00:00:00Z",
};

describe("filterAssets", () => {
  it("filters assets by query, kind, and source before sorting", () => {
    const assets: Asset[] = [
      {
        ...baseAsset,
        discovered_at: "2026-01-02T00:00:00Z",
        id: "skill-a",
        kind: "skill",
        name: "Codex Review Skill",
        source_id: "source-a",
      },
      {
        ...baseAsset,
        discovered_at: "2026-01-03T00:00:00Z",
        id: "prompt-b",
        kind: "prompt",
        name: "Codex Review Prompt",
        source_id: "source-a",
      },
      {
        ...baseAsset,
        discovered_at: "2026-01-04T00:00:00Z",
        id: "skill-c",
        kind: "skill",
        name: "Codex Review Skill Copy",
        source_id: "source-b",
      },
    ];

    expect(
      filterAssets(assets, {
        kindFilters: ["skill"],
        query: "review",
        sortBy: "created",
        sortDirection: "desc",
        sourceFilters: ["source-a"],
      }).map((asset) => asset.id),
    ).toEqual(["skill-a"]);
  });

  it("sorts assets by updated time and name with stable fallbacks", () => {
    const assets: Asset[] = [
      { ...baseAsset, id: "alpha", name: "Alpha", updated_at: "2026-01-02T00:00:00Z" },
      { ...baseAsset, id: "charlie", name: "Charlie", updated_at: "2026-01-03T00:00:00Z" },
      { ...baseAsset, id: "bravo", name: "Bravo", updated_at: "2026-01-03T00:00:00Z" },
    ];

    expect(
      filterAssets(assets, {
        kindFilters: [],
        query: "",
        sortBy: "updated",
        sortDirection: "desc",
        sourceFilters: [],
      }).map((asset) => asset.id),
    ).toEqual(["bravo", "charlie", "alpha"]);
  });
});
