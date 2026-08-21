import { useState } from "react";
import {
  createPlan,
  executePlan,
  scanSources,
  waitForSourceScanTask,
  type SourceScanScope,
  type SourceScanTaskSnapshot,
} from "../../services/catalog";
import type { Asset, AssetKind, DeploymentPlan, ExecutionResult } from "../../types";

export function useCatalogOperations(
  refreshOverview: (nextAssets?: Asset[]) => Promise<void>,
  activeAssetKind?: AssetKind,
  startBackgroundScan?: (
    kind?: Extract<AssetKind, "skill" | "prompt" | "rule">,
    scope?: SourceScanScope,
  ) => Promise<SourceScanTaskSnapshot>,
) {
  const [plan, setPlan] = useState<DeploymentPlan | null>(null);
  const [executionResult, setExecutionResult] = useState<ExecutionResult | null>(null);
  const [busy, setBusy] = useState(false);

  async function scan() {
    setBusy(true);
    try {
      if (startBackgroundScan) {
        const scanKind =
          activeAssetKind === "skill" || activeAssetKind === "prompt" || activeAssetKind === "rule"
            ? activeAssetKind
            : undefined;
        const task = await startBackgroundScan(scanKind, "all");
        void settleSourceScan(task).catch(() => refreshOverview());
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

  async function settleSourceScan(task: SourceScanTaskSnapshot) {
    const terminal = isTerminalSourceScan(task) ? task : await waitForSourceScanTask(task.id);
    if (terminal.status === "completed" && terminal.result) {
      await refreshOverview(terminal.result);
      setPlan(null);
      setExecutionResult(null);
    } else {
      await refreshOverview();
    }
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

function isTerminalSourceScan(task: SourceScanTaskSnapshot) {
  return task.status === "completed" || task.status === "failed" || task.status === "cancelled";
}
