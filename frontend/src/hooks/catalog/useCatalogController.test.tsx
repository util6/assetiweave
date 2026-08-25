// @vitest-environment jsdom

import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BatchMountTaskSnapshot } from "../../services/catalog";

const fixtures = vi.hoisted(() => ({
  batchMount: null as BatchMountTaskSnapshot | null,
  clearDeploymentPlan: vi.fn(),
  refreshMountState: vi.fn(),
  setMountProfiles: vi.fn(),
}));

vi.mock("../../app/backgroundTasks/CatalogTaskProvider", () => ({
  useCatalogTasks: () => ({
    batchMount: fixtures.batchMount,
    sourceScan: null,
    startBatchMount: vi.fn(),
    startSourceScan: vi.fn(),
  }),
}));
vi.mock("../../store/settings/AppSettingsProvider", () => ({
  useAppSettings: () => ({ settings: { showStartupNotification: false } }),
}));
vi.mock("../tenants/useTenantController", () => ({ useTenantController: () => ({}) }));
vi.mock("./useAssetFilter", () => ({ useAssetFilter: () => [] }));
vi.mock("./useCatalogData", () => ({
  useCatalogData: () => ({
    activeAssetKind: "skill",
    applyAssetMountStatus: vi.fn(),
    assetMountStatuses: [],
    assets: [{ id: "asset-1", name: "Asset 1", source_id: "source-1" }],
    profiles: [{ id: "profile-1", name: "Profile 1" }],
    refreshCatalogAndMountState: vi.fn(),
    refreshMountState: fixtures.refreshMountState,
    refreshOverview: vi.fn(),
    reloadCatalogData: vi.fn(),
    sources: [{ id: "source-1", source_origin: "local_folder" }],
  }),
}));
vi.mock("./useCatalogOperations", () => ({
  useCatalogOperations: () => ({ clearDeploymentPlan: fixtures.clearDeploymentPlan }),
}));
vi.mock("./useExpandedAssets", () => ({
  useExpandedAssets: () => ({ expandedIds: new Set(), toggleAsset: vi.fn() }),
}));
vi.mock("./useMountSelection", () => ({
  useMountSelection: () => ({
    setMountProfiles: fixtures.setMountProfiles,
    toggleMountProfile: vi.fn(),
  }),
}));

import { useCatalogController } from "./useCatalogController";

describe("useCatalogController background mounts", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    fixtures.batchMount = null;
    fixtures.refreshMountState.mockResolvedValue([]);
    fixtures.setMountProfiles.mockResolvedValue(runningBatchTask());
  });

  it("does not refresh or report success before a background batch reaches terminal state", async () => {
    const { result, rerender } = renderHook(() => useCatalogController());

    await act(async () => {
      await result.current.setMountProfiles(["asset-1"], "profile-1", true);
    });

    expect(fixtures.refreshMountState).not.toHaveBeenCalled();
    expect(result.current.notification).toBeNull();

    fixtures.batchMount = {
      ...runningBatchTask(),
      status: "completed",
      progress: { phase: "completed", completed: 1, total: 1, current_id: null },
      finished_at: "2026-08-23T00:00:01Z",
      result: {},
    };
    rerender();

    await waitFor(() => expect(fixtures.refreshMountState).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(result.current.notification?.tone).toBe("success"));
    expect(fixtures.clearDeploymentPlan).toHaveBeenCalledTimes(1);
  });

  it("settles a task that is already terminal when start returns", async () => {
    fixtures.setMountProfiles.mockResolvedValue({
      ...runningBatchTask(),
      status: "completed",
      finished_at: "2026-08-23T00:00:01Z",
      result: {},
    });
    const { result } = renderHook(() => useCatalogController());

    await act(async () => {
      await result.current.setMountProfiles(["asset-1"], "profile-1", true);
    });

    await waitFor(() => expect(fixtures.refreshMountState).toHaveBeenCalledTimes(1));
    expect(result.current.notification?.tone).toBe("success");
  });
});

function runningBatchTask(): BatchMountTaskSnapshot {
  return {
    id: "batch-1",
    status: "running",
    mode: "explicit",
    profile_id: "profile-1",
    progress: { phase: "applying", completed: 0, total: 1, current_id: null },
    started_at: "2026-08-23T00:00:00Z",
    finished_at: null,
    result: null,
    error: null,
  };
}
