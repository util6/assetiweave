import { useState } from "react";
import { createPlan, executePlan, scanSources } from "../../services/catalog";
import type { Asset, DeploymentPlan, ExecutionResult } from "../../types";

export function useCatalogOperations(refreshOverview: (nextAssets?: Asset[]) => Promise<void>) {
  const [plan, setPlan] = useState<DeploymentPlan | null>(null);
  const [executionResult, setExecutionResult] = useState<ExecutionResult | null>(null);
  const [busy, setBusy] = useState(false);

  async function scan() {
    setBusy(true);
    try {
      const scannedAssets = await scanSources();
      await refreshOverview(scannedAssets);
      setPlan(null);
      setExecutionResult(null);
    } finally {
      setBusy(false);
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
