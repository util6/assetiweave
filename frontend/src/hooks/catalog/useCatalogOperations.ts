import { useEffect, useRef, useState } from "react";
import {
  scanSources,
  type SourceScanScope,
} from "../../services/catalog";
import type { TaskView } from "../../types/taskCenter";
import type {
  Asset,
  AssetKind,
} from "../../types";

export function useCatalogOperations(
  refreshOverview: (nextAssets?: Asset[]) => Promise<void>,
  activeAssetKind?: AssetKind,
  startBackgroundScan?: (
    kind?: Extract<AssetKind, "skill" | "prompt" | "rule">,
    scope?: SourceScanScope,
  ) => Promise<TaskView>,
  sourceScan?: TaskView | null,
) {
  const [busy, setBusy] = useState(false);
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
    void settleSourceScan(sourceScan).catch(() => refreshOverview());
  }, [sourceScan]);

  async function scan() {
    setBusy(true);
    try {
      if (startBackgroundScan) {
        const scanKind =
          activeAssetKind === "skill" ||
          activeAssetKind === "prompt" ||
          activeAssetKind === "rule"
            ? activeAssetKind
            : undefined;
        const task = await startBackgroundScan(scanKind, "all");
        startedScanIdsRef.current.add(task.id);
        if (isTerminalSourceScan(task)) {
          settledScanIdsRef.current.add(task.id);
          await settleSourceScan(task);
        }
        return;
      }
      const scannedAssets = await scanSources(activeAssetKind);
      await refreshOverview(scannedAssets);
    } finally {
      setBusy(false);
    }
  }

  async function settleSourceScan(_task: TaskView) {
    await refreshOverview();
  }

  return {
    busy,
    scan,
  };
}

function isTerminalSourceScan(task: TaskView) {
  return (
    task.state === "succeeded" ||
    task.state === "failed" ||
    task.state === "canceled"
  );
}
