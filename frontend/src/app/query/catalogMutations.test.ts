/* @vitest-environment jsdom */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { act, renderHook, waitFor } from "@testing-library/react";
import { createElement, type ReactNode } from "react";

import { describe, expect, it, vi } from "vitest";
import {
  invalidateCatalog,
  useUpdateAssetDescription,
} from "./catalogMutations";
import { catalogKeys } from "./catalogQueries";
import type { Asset } from "../../types";

const updateAssetDescriptionMock = vi.hoisted(() => vi.fn());

vi.mock("../../services/catalog", () => ({
  updateAssetDescription: updateAssetDescriptionMock,
  updateNavigationModel: vi.fn(),
  updateAppShortcuts: vi.fn(),
}));

describe("catalogMutations", () => {
  it("批量完成每个资源只失效一次", async () => {
    const client = new QueryClient();
    const invalidate = vi
      .spyOn(client, "invalidateQueries")
      .mockResolvedValue();
    const scope = { tenantId: "workspace-a", epoch: 1 };
    await invalidateCatalog(client, scope, [
      "mountStatuses",
      "overview",
      "mountStatuses",
    ]);
    expect(invalidate).toHaveBeenCalledTimes(2);
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: catalogKeys.mountStatuses(scope),
    });
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: catalogKeys.overview(scope),
    });
    client.clear();
  });

  it("写失败不伪造成功并保留错误状态", async () => {
    const client = new QueryClient({
      defaultOptions: { mutations: { retry: false } },
    });
    updateAssetDescriptionMock.mockRejectedValueOnce(
      new Error("IPC failed to update description"),
    );

    const wrapper = ({ children }: { children: ReactNode }) =>
      createElement(QueryClientProvider, { client }, children);

    const scope = { tenantId: "workspace-a", epoch: 1 };
    const { result } = renderHook(() => useUpdateAssetDescription(scope), {
      wrapper,
    });

    await act(async () => {
      await expect(
        result.current.mutateAsync({
          assetId: "asset-1",
          description: "New description",
        }),
      ).rejects.toThrow("IPC failed to update description");
    });

    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });
    expect(result.current.error?.message).toBe(
      "IPC failed to update description",
    );
  });

  it("旧 tenant 的 mutation 成功不写入当前新 tenant 的 cache", async () => {
    const client = new QueryClient({
      defaultOptions: { mutations: { retry: false } },
    });

    const tenantAAsset: Asset = {
      id: "asset-1",
      name: "Skill 1",
      kind: "skill",
      format: "directory",
      relative_path: "skills/skill-1",
      absolute_path: "/test/skills/skill-1",
      description: "Old description",
      source_id: "src-1",
      discovered_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
    };

    const tenantBAsset: Asset = {
      id: "asset-1",
      name: "Skill 1 in Tenant B",
      kind: "skill",
      format: "directory",
      relative_path: "skills/skill-1",
      absolute_path: "/test/skills/skill-1",
      description: "Tenant B description",
      source_id: "src-2",
      discovered_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
    };

    const scopeA = { tenantId: "tenant-a", epoch: 1 };
    const scopeB = { tenantId: "tenant-b", epoch: 2 };

    // Set initial data in both scopes
    client.setQueryData(catalogKeys.assets(scopeA, "skill"), [tenantAAsset]);
    client.setQueryData(catalogKeys.assets(scopeB, "skill"), [tenantBAsset]);

    const updatedA: Asset = {
      ...tenantAAsset,
      description: "Updated by late mutation for tenant A",
    };
    updateAssetDescriptionMock.mockResolvedValueOnce(updatedA);

    const wrapper = ({ children }: { children: ReactNode }) =>
      createElement(QueryClientProvider, { client }, children);

    // Mutation created with old scopeA

    const { result } = renderHook(() => useUpdateAssetDescription(scopeA), {
      wrapper,
    });

    await act(async () => {
      await result.current.mutateAsync({
        assetId: "asset-1",
        description: "Updated by late mutation for tenant A",
      });
    });

    // Verify tenant B data is untouched
    const tenantBData = client.getQueryData<Asset[]>(
      catalogKeys.assets(scopeB, "skill"),
    );
    expect(tenantBData?.[0].description).toBe("Tenant B description");

    // Verify tenant A data received the update
    const tenantAData = client.getQueryData<Asset[]>(
      catalogKeys.assets(scopeA, "skill"),
    );
    expect(tenantAData?.[0].description).toBe(
      "Updated by late mutation for tenant A",
    );
  });
});
