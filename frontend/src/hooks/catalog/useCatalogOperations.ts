import { useEffect, useRef, useState } from "react";
import {
  createPlan,
  executePlan,
  scanSources,
  type SourceScanScope,
} from "../../services/catalog";
import type { TaskView } from "../../types/taskCenter";
import type {
  Asset,
  AssetKind,
  DeploymentPlan,
  ExecutionResult,
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
  const [plan, setPlan] = useState<DeploymentPlan | null>(null);
  const [executionResult, setExecutionResult] =
    useState<ExecutionResult | null>(null);
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
      setPlan(null);
      setExecutionResult(null);
    } finally {
      setBusy(false);
    }
  }

  async function settleSourceScan(_task: TaskView) {
    await refreshOverview();
    setPlan(null);
    setExecutionResult(null);
  }

  async function createDeploymentPlan() {
    setBusy(true);
    try {
      setPlan(await createPlan());
      setExecutionResult(null);
    } finally {
      setBusy(false);
    }
  }

  async function executeDeploymentPlan() {
    if (!plan) return;
    setBusy(true);
    try {
      setExecutionResult(await executePlan(plan));
      await refreshOverview();
    } finally {
      setBusy(false);
    }
  }

  function clearDeploymentPlan() {
    setPlan(null);
    setExecutionResult(null);
  }

  return {
    busy,
    clearDeploymentPlan,
    createDeploymentPlan,
    executeDeploymentPlan,
    executionResult,
    plan,
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
