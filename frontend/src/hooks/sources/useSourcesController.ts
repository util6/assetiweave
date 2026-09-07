import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useQueryScope } from "../../app/query/QueryScopeProvider";
import {
  catalogKeys,
  skillAssetsQueryOptions,
  skillSourcesQueryOptions,
} from "../../app/query/catalogQueries";
import {
  createSource,
  deleteSource as deleteSourceById,
  revealPath,
  scanSkillSources,
  updateSource,
} from "../../services/catalog";
import type { Asset, Source, SourceInput } from "../../types";
import type {
  SourceScanScope,
  SourceScanTaskSnapshot,
} from "../../services/catalog";

export function useSourcesController(
  onCatalogRefresh?: (assets?: Asset[]) => Promise<void>,
  startBackgroundScan?: (
    kind?: "skill" | "prompt" | "rule",
    scope?: SourceScanScope,
  ) => Promise<SourceScanTaskSnapshot>,
  sourceScan?: SourceScanTaskSnapshot | null,
) {
  const queryClient = useQueryClient();
  const queryScope = useQueryScope();
  const activeScope = queryScope ?? { tenantId: "default", epoch: 1 };

  const sourcesQuery = useQuery(skillSourcesQueryOptions(activeScope));
  const sourceAssetsQuery = useQuery(skillAssetsQueryOptions(activeScope));

  const sources = sourcesQuery.data ?? [];
  const sourceAssets = sourceAssetsQuery.data ?? [];
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const loading = sourcesQuery.isLoading || sourceAssetsQuery.isLoading;

  const startedScanIdsRef = useRef(new Set<string>());
  const settledScanIdsRef = useRef(new Set<string>());

  useEffect(() => {
    if (
      !sourceScan ||
      !isTerminalSourceScan(sourceScan) ||
      !startedScanIdsRef.current.has(sourceScan.id) ||
      settledScanIdsRef.current.has(sourceScan.id)
    ) {
      return;
    }
    settledScanIdsRef.current.add(sourceScan.id);
    void refreshAfterSourceScan(sourceScan).catch(() => onCatalogRefresh?.());
  }, [sourceScan]);

  const assetCounts = useMemo(() => {
    return sourceAssets.reduce<Record<string, number>>((counts, asset) => {
      counts[asset.source_id] = (counts[asset.source_id] ?? 0) + 1;
      return counts;
    }, {});
  }, [sourceAssets]);

  const filteredSources = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    if (!normalizedQuery) {
      return sources;
    }

    return sources.filter((source) => {
      const searchable = [
        source.name,
        source.root_path,
        source.kind,
        source.default_kind ?? "",
        source.last_scan_status ?? "",
        ...source.include_globs,
        ...source.exclude_globs,
      ]
        .join(" ")
        .toLowerCase();
      return searchable.includes(normalizedQuery);
    });
  }, [query, sources]);

  const summary = useMemo(() => {
    return {
      total: sources.length,
      enabled: sources.filter((source) => source.enabled).length,
      assets: Object.values(assetCounts).reduce(
        (total, count) => total + count,
        0,
      ),
      issues: sources.filter((source) =>
        source.last_scan_status?.startsWith("error:"),
      ).length,
    };
  }, [assetCounts, sources]);

  const nextPriority = useMemo(
    () =>
      sources.reduce(
        (highest, source) => Math.max(highest, source.priority),
        -10,
      ) + 10,
    [sources],
  );

  async function refreshSources() {
    await queryClient.invalidateQueries({
      queryKey: catalogKeys.skillSources(activeScope),
    });
    return await queryClient.fetchQuery(skillSourcesQueryOptions(activeScope));
  }

  async function refreshSourceAssets() {
    await queryClient.invalidateQueries({
      queryKey: catalogKeys.skillAssets(activeScope),
    });
    return await queryClient.fetchQuery(skillAssetsQueryOptions(activeScope));
  }

  async function toggleSource(source: Source) {
    setBusy(true);
    try {
      const saved = await updateSource({ ...source, enabled: !source.enabled });
      queryClient.setQueryData<Source[]>(
        catalogKeys.skillSources(activeScope),
        (current = []) =>
          current.map((candidate) =>
            candidate.id === saved.id ? saved : candidate,
          ),
      );
    } finally {
      setBusy(false);
    }
  }

  async function removeSource(source: Source) {
    setBusy(true);
    try {
      await deleteSourceById(source.id);
      queryClient.setQueryData<Source[]>(
        catalogKeys.skillSources(activeScope),
        (current = []) =>
          current.filter((candidate) => candidate.id !== source.id),
      );
      queryClient.setQueryData<Asset[]>(
        catalogKeys.skillAssets(activeScope),
        (current = []) =>
          current.filter((candidate) => candidate.source_id !== source.id),
      );
      await onCatalogRefresh?.();
    } finally {
      setBusy(false);
    }
  }

  async function saveSource(source: Source) {
    setBusy(true);
    try {
      const saved = await updateSource(source);
      queryClient.setQueryData<Source[]>(
        catalogKeys.skillSources(activeScope),
        (current = []) => upsertAndSortSources(current, saved),
      );
      if (saved.enabled && saved.last_scan_status !== "preview") {
        await startSkillScan();
      } else {
        await onCatalogRefresh?.();
      }
    } finally {
      setBusy(false);
    }
  }

  async function importSource(sourceInput: SourceInput) {
    setBusy(true);
    try {
      const saved = await createSource(sourceInput);
      queryClient.setQueryData<Source[]>(
        catalogKeys.skillSources(activeScope),
        (current = []) => upsertAndSortSources(current, saved),
      );
      if (saved.enabled && saved.last_scan_status !== "preview") {
        await startSkillScan();
      } else {
        await onCatalogRefresh?.();
      }
    } finally {
      setBusy(false);
    }
  }

  async function scanAllSources() {
    setBusy(true);
    try {
      await startSkillScan();
    } finally {
      setBusy(false);
    }
  }

  async function startSkillScan() {
    if (!startBackgroundScan) {
      const scannedAssets = await scanSkillSources();
      await onCatalogRefresh?.(scannedAssets);
      await Promise.all([refreshSources(), refreshSourceAssets()]);
      return;
    }

    const task = await startBackgroundScan("skill", "skills");
    startedScanIdsRef.current.add(task.id);
    if (isTerminalSourceScan(task)) {
      settledScanIdsRef.current.add(task.id);
      await refreshAfterSourceScan(task);
    }
  }

  async function refreshAfterSourceScan(task: SourceScanTaskSnapshot) {
    if (task.status === "completed" && task.result) {
      await onCatalogRefresh?.(task.result);
    } else {
      await onCatalogRefresh?.();
    }
    await Promise.all([refreshSources(), refreshSourceAssets()]);
  }

  return {
    applySourceAssetUpdate: (asset: Asset) =>
      queryClient.setQueryData<Asset[]>(
        catalogKeys.skillAssets(activeScope),
        (current = []) =>
          current.map((candidate) =>
            candidate.id === asset.id ? asset : candidate,
          ),
      ),
    assetCounts,
    busy,
    filteredSources,
    importSource,
    loading,
    nextPriority,
    query,
    revealPath,
    removeSource,
    saveSource,
    scanAllSources,
    setQuery,
    sources,
    sourceAssets,
    summary,
    toggleSource,
    removeSourceAsset: (assetId: string) =>
      queryClient.setQueryData<Asset[]>(
        catalogKeys.skillAssets(activeScope),
        (current = []) => current.filter((asset) => asset.id !== assetId),
      ),
  };
}

function isTerminalSourceScan(task: SourceScanTaskSnapshot) {
  return (
    task.status === "completed" ||
    task.status === "failed" ||
    task.status === "cancelled"
  );
}

function upsertAndSortSources(sources: Source[], source: Source) {
  return [
    ...sources.filter((candidate) => candidate.id !== source.id),
    source,
  ].sort((left, right) => {
    const priorityOrder = left.priority - right.priority;
    return priorityOrder === 0
      ? left.name.localeCompare(right.name)
      : priorityOrder;
  });
}
