// @vitest-environment jsdom

import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Asset, Source } from "../../types";

const catalogService = vi.hoisted(() => ({
  createSource: vi.fn(),
  deleteSource: vi.fn(),
  listSkillSources: vi.fn(),
  listSourceAssets: vi.fn(),
  revealPath: vi.fn(),
  scanSkillSources: vi.fn(),
  updateSource: vi.fn(),
}));

vi.mock("../../services/catalog", () => catalogService);

import { useSourcesController } from "./useSourcesController";

describe("useSourcesController", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("keeps duplicate source copies visible after a scan refreshes the global catalog", async () => {
    const source = createSource("local-system-copy");
    const sourceAsset = createAsset("local-system-asset", source.id);
    const canonicalAsset = createAsset("system-asset", "assetiweave-system-skills");
    const onCatalogRefresh = vi.fn().mockResolvedValue(undefined);
    catalogService.listSkillSources.mockResolvedValue([source]);
    catalogService.listSourceAssets.mockResolvedValue([sourceAsset]);
    catalogService.scanSkillSources.mockResolvedValue([canonicalAsset]);

    const { result } = renderHook(() => useSourcesController(onCatalogRefresh));

    await waitFor(() => expect(result.current.sourceAssets).toEqual([sourceAsset]));

    await act(async () => {
      await result.current.scanAllSources();
    });

    expect(onCatalogRefresh).toHaveBeenCalledWith([canonicalAsset]);
    expect(result.current.sourceAssets).toEqual([sourceAsset]);
  });
});

function createSource(id: string): Source {
  return {
    id,
    name: ".system",
    kind: "local",
    root_path: "~/.assetiweave/skills/.system",
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
    last_scan_status: "ok: 4 assets",
  };
}

function createAsset(id: string, sourceId: string): Asset {
  return {
    id,
    source_id: sourceId,
    name: "assetiweave-memory",
    kind: "skill",
    format: "directory",
    relative_path: "assetiweave-memory",
    absolute_path: "~/.assetiweave/skills/.system/assetiweave-memory",
    entry_file: "SKILL.md",
    description: null,
    content_hash: "same-content",
    discovered_at: "2026-07-28T00:00:00Z",
    updated_at: "2026-07-28T00:00:00Z",
  };
}
