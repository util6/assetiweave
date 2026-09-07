import { useEffect, useRef } from "react";
import {
  useMutation,
  useQueryClient,
  type QueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import type { NavigationModel } from "../../router/types";
import {
  updateAppShortcuts,
  updateAssetDescription,
  updateNavigationModel,
} from "../../services/catalog";
import type { AppShortcut, Asset } from "../../types";
import { catalogKeys, type QueryScope } from "./catalogQueries";

export type CatalogInvalidation =
  | "assets"
  | "sources"
  | "profiles"
  | "overview"
  | "mountStatuses"
  | "shortcuts"
  | "groups"
  | "skillSources"
  | "skillAssets";

export async function invalidateCatalog(
  client: QueryClient,
  scope: QueryScope,
  resources: readonly CatalogInvalidation[],
): Promise<void> {
  const uniqueResources = new Set(resources);
  const tasks: Promise<void>[] = [];

  for (const resource of uniqueResources) {
    switch (resource) {
      case "assets":
        tasks.push(
          client.invalidateQueries({
            queryKey: catalogKeys.assetsPrefix(scope),
          }),
        );
        break;
      case "sources":
        tasks.push(
          client.invalidateQueries({ queryKey: catalogKeys.sources(scope) }),
        );
        break;
      case "profiles":
        tasks.push(
          client.invalidateQueries({ queryKey: catalogKeys.profiles(scope) }),
        );
        break;
      case "overview":
        tasks.push(
          client.invalidateQueries({ queryKey: catalogKeys.overview(scope) }),
        );
        break;
      case "mountStatuses":
        tasks.push(
          client.invalidateQueries({
            queryKey: catalogKeys.mountStatuses(scope),
          }),
        );
        break;
      case "shortcuts":
        tasks.push(
          client.invalidateQueries({ queryKey: catalogKeys.shortcuts(scope) }),
        );
        break;
      case "groups":
        tasks.push(
          client.invalidateQueries({ queryKey: catalogKeys.groups(scope) }),
        );
        break;
      case "skillSources":
        tasks.push(
          client.invalidateQueries({
            queryKey: catalogKeys.skillSources(scope),
          }),
        );
        break;
      case "skillAssets":
        tasks.push(
          client.invalidateQueries({
            queryKey: catalogKeys.skillAssets(scope),
          }),
        );
        break;
    }
  }

  await Promise.all(tasks);
}

export function useUpdateAssetDescription(
  scope: QueryScope,
): UseMutationResult<
  Asset,
  Error,
  { assetId: string; description: string | null }
> {
  const queryClient = useQueryClient();
  return useMutation({
    networkMode: "always",
    mutationFn: ({
      assetId,
      description,
    }: {
      assetId: string;
      description: string | null;
    }) => updateAssetDescription(assetId, description),
    onSuccess: (updatedAsset) => {
      queryClient.setQueriesData<Asset[]>(
        { queryKey: catalogKeys.assetsPrefix(scope) },
        (current) =>
          current?.map((asset) =>
            asset.id === updatedAsset.id ? updatedAsset : asset,
          ),
      );
    },
  });
}

export function useSaveNavigation(
  scope: QueryScope,
  options?: {
    onSettled?: (result: { success: boolean; model: NavigationModel }) => void;
  },
): {
  save(model: NavigationModel): Promise<NavigationModel>;
  schedule(model: NavigationModel): void;
} {
  const queryClient = useQueryClient();
  const sequenceRef = useRef(0);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const mutation = useMutation({
    mutationKey: catalogKeys.navigation(scope),
    networkMode: "always",
    scope: { id: JSON.stringify(catalogKeys.navigation(scope)) },
    mutationFn: async ({
      model,
      sequence,
    }: {
      model: NavigationModel;
      sequence: number;
    }) => {
      const saved = await updateNavigationModel(model);
      return { saved, sequence };
    },
    onSuccess: ({ saved, sequence }) => {
      if (sequenceRef.current === sequence) {
        queryClient.setQueryData(catalogKeys.navigation(scope), saved);
      }
    },
    onSettled: (data, error, variables) => {
      options?.onSettled?.({
        success: !error,
        model: data?.saved ?? variables.model,
      });
    },
  });

  useEffect(() => {
    return () => {
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    };
  }, [scope.tenantId, scope.epoch]);

  function schedule(model: NavigationModel) {
    const sequence = ++sequenceRef.current;
    queryClient.setQueryData(catalogKeys.navigation(scope), model);
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
    }
    timerRef.current = setTimeout(() => {
      timerRef.current = null;
      mutation.mutate({ model, sequence });
    }, 120);
  }

  async function save(model: NavigationModel): Promise<NavigationModel> {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    const sequence = ++sequenceRef.current;
    queryClient.setQueryData(catalogKeys.navigation(scope), model);
    const { saved } = await mutation.mutateAsync({ model, sequence });
    return saved;
  }

  return { save, schedule };
}

export function useSaveAppShortcuts(scope: QueryScope) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationKey: catalogKeys.shortcuts(scope),
    networkMode: "always",
    mutationFn: async (shortcuts: AppShortcut[]) => {
      queryClient.setQueryData(catalogKeys.shortcuts(scope), shortcuts);
      return await updateAppShortcuts(shortcuts);
    },
    onSuccess: (savedShortcuts) => {
      queryClient.setQueryData(catalogKeys.shortcuts(scope), savedShortcuts);
    },
  });
}
