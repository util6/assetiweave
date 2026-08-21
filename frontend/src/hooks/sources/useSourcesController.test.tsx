// @vitest-environment jsdom

import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { clearSharedResourceCache } from "../../lib/asyncCache";
import type { Asset, Source } from "../../types";

const catalogService = vi.hoisted(() => ({
  createSource: vi.fn(),
  deleteSource: vi.fn(),
  listSkillSources: vi.fn(),
  listSourceAssets: vi.fn(),
  revealPath: vi.fn(),
  scanSkillSources: vi.fn(),
  startSourceScan: vi.fn(),
  updateSource: vi.fn(),
  waitForSourceScanTask: vi.fn(),
}));

vi.mock("../../services/catalog", () => catalogService);

import { useSourcesController } from "./useSourcesController";

describe("useSourcesController", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearSharedResourceCache();
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

  it("returns after background scan start and refreshes when the task settles", async () => {
    const source = createSource("local-system-copy");
    catalogService.listSkillSources.mockResolvedValue([source]);
    catalogService.listSourceAssets.mockResolvedValue([]);
    const onCatalogRefresh = vi.fn().mockResolvedValue(undefined);
    const task = runningSourceScan();
    const terminal = { ...task, status: "completed", result: [], finished_at: "2026-08-21T00:00:02Z" };
    const startBackgroundScan = vi.fn().mockResolvedValue(task);
    catalogService.waitForSourceScanTask.mockResolvedValue(terminal);

    const { result } = renderHook(() => useSourcesController(onCatalogRefresh, startBackgroundScan));
    await waitFor(() => expect(result.current.sources).toEqual([source]));

    await act(async () => {
      await result.current.scanAllSources();
    });

    expect(startBackgroundScan).toHaveBeenCalledWith("skill", "skills");
    await waitFor(() => expect(onCatalogRefresh).toHaveBeenCalledWith([]));
  });
});

function runningSourceScan() {
  return {
    id: "scan-task",
    status: "running" as const,
    scope: "skills" as const,
    kind: "skill" as const,
    progress: {
      phase: "scanning" as const,
      completed_source_count: 0,
      total_source_count: 1,
      current_source_name: null,
    },
    started_at: "2026-08-21T00:00:00Z",
    finished_at: null,
    result: null,
    error: null,
  };
}

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
