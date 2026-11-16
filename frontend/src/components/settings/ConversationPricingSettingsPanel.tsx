import React, { useState, useEffect, useCallback } from "react";
import {
  Coins,
  RefreshCw,
  Check,
  AlertCircle,
  CheckCircle2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Panel } from "@/components/foundation/Panel";
import { Badge } from "@/components/foundation/Badge";
import { AppSkeleton } from "@/components/foundation/skeleton";
import type {
  ModelPriceEntry,
  ModelPricingCatalog,
  ModelPricingStatus,
} from "@/types/usage";
import {
  getModelPricingCatalog,
  getModelPricingStatus,
  rebindModelPricingAlias,
  saveModelPriceEntry,
  setModelPricingExchangeRate,
  setModelPricingIgnored,
  setModelPricingLock,
  subscribeModelPricingSyncTask,
  updateModelPricingCatalog,
} from "@/services/usage";
import { ModelPricingTable } from "./ModelPricingTable";

export const ConversationPricingSettingsPanel: React.FC = () => {
  const [loading, setLoading] = useState<boolean>(true);
  const [status, setStatus] = useState<ModelPricingStatus | null>(null);
  const [catalog, setCatalog] = useState<ModelPricingCatalog | null>(null);

  // Exchange rate states
  const [rateInput, setRateInput] = useState<string>("7.25");
  const [savingRate, setSavingRate] = useState<boolean>(false);
  const [rateNotice, setRateNotice] = useState<{
    type: "success" | "error";
    text: string;
  } | null>(null);

  // Sync pricing states
  const [syncing, setSyncing] = useState<boolean>(false);
  const [syncNotice, setSyncNotice] = useState<{
    type: "success" | "error";
    text: string;
  } | null>(null);

  // Model save state
  const [savingModel, setSavingModel] = useState<boolean>(false);
  const [modelNotice, setModelNotice] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const [st, cat] = await Promise.all([
        getModelPricingStatus(),
        getModelPricingCatalog(),
      ]);
      setStatus(st);
      setCatalog(cat);
      setRateInput(String(st.exchangeRate || 7.25));
    } catch (err) {
      setSyncNotice({
        type: "error",
        text: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  // Subscribe to real-time pricing sync task events
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    void subscribeModelPricingSyncTask((snapshot) => {
      if (cancelled) return;
      const isRunning =
        snapshot.status === "running" || snapshot.status === "cancelling";

      setSyncing(isRunning);
      if (!isRunning) {
        if (snapshot.status === "completed") {
          const detail = snapshot.result;
          setSyncNotice({
            type: "success",
            text: detail
              ? `价格同步完成：更新 ${detail.modelsCount} 款模型，重算 ${detail.recalculatedEventsCount} 条会话用量（汇率 1 USD = ¥${detail.exchangeRate.toFixed(2)}）`
              : "价格同步完成，已重算历史用量",
          });
          void loadData();
        } else if (snapshot.status === "failed") {
          setSyncNotice({
            type: "error",
            text: snapshot.error?.message || "网络同步模型价格失败",
          });
        }
      }
    })
      .then((cleanup) => {
        if (cancelled) cleanup();
        else unlisten = cleanup;
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [loadData]);

  // Save exchange rate
  const handleSaveRate = async () => {
    const val = parseFloat(rateInput);
    if (isNaN(val) || val <= 0) {
      setRateNotice({ type: "error", text: "请输入合法的正数汇率" });
      return;
    }
    setSavingRate(true);
    setRateNotice(null);
    try {
      const updated = await setModelPricingExchangeRate(val);
      setStatus(updated);
      setRateNotice({
        type: "success",
        text: `汇率已更新为 1 USD = ¥${val.toFixed(2)}，并已重算历史费用`,
      });
      const cat = await getModelPricingCatalog();
      setCatalog(cat);
    } catch (err) {
      setRateNotice({
        type: "error",
        text: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setSavingRate(false);
    }
  };

  // Trigger online pricing update
  const handleTriggerSync = async () => {
    setSyncing(true);
    setSyncNotice(null);
    try {
      const snapshot = await updateModelPricingCatalog(true);
      if (snapshot.status === "completed") {
        setSyncNotice({
          type: "success",
          text: `同步完成：收录 ${snapshot.result?.modelsCount ?? 0} 款模型单价`,
        });
        await loadData();
        setSyncing(false);
      }
    } catch (err) {
      setSyncNotice({
        type: "error",
        text: err instanceof Error ? err.message : String(err),
      });
      setSyncing(false);
    }
  };

  // Save updated model entry (auto-locks base model)
  const handleSaveModel = async (updated: ModelPriceEntry) => {
    setSavingModel(true);
    setModelNotice(null);
    try {
      await saveModelPriceEntry(updated);
      const targetName =
        updated.matchedModelId || updated.rawModel || updated.modelPatterns[0];
      setModelNotice(
        `模型【${targetName}】单价已保存并锁定（🔒），已重算历史用量`,
      );
      const [cat, st] = await Promise.all([
        getModelPricingCatalog(),
        getModelPricingStatus(),
      ]);
      setCatalog(cat);
      setStatus(st);
    } catch (err) {
      setModelNotice(err instanceof Error ? err.message : String(err));
      throw err;
    } finally {
      setSavingModel(false);
    }
  };

  const handleRebindModel = async (
    rawModel: string,
    targetModelId: string | null,
  ) => {
    setSavingModel(true);
    setModelNotice(null);
    try {
      await rebindModelPricingAlias(rawModel, targetModelId);
      setModelNotice(
        targetModelId
          ? `已将【${rawModel}】手动改绑至底座模型【${targetModelId}】，并重算历史用量`
          : `已解除【${rawModel}】的手动改绑，恢复自动匹配`,
      );
      const [cat, st] = await Promise.all([
        getModelPricingCatalog(),
        getModelPricingStatus(),
      ]);
      setCatalog(cat);
      setStatus(st);
    } catch (err) {
      setModelNotice(err instanceof Error ? err.message : String(err));
      throw err;
    } finally {
      setSavingModel(false);
    }
  };

  const handleSetPriceLock = async (modelId: string, locked: boolean) => {
    setSavingModel(true);
    setModelNotice(null);
    try {
      await setModelPricingLock(modelId, locked);
      setModelNotice(
        locked
          ? `已锁定底座模型【${modelId}】单价`
          : `已解锁底座模型【${modelId}】，立即回落全网参考单价并重算历史用量`,
      );
      const [cat, st] = await Promise.all([
        getModelPricingCatalog(),
        getModelPricingStatus(),
      ]);
      setCatalog(cat);
      setStatus(st);
    } catch (err) {
      setModelNotice(err instanceof Error ? err.message : String(err));
      throw err;
    } finally {
      setSavingModel(false);
    }
  };

  const handleSetModelIgnored = async (
    rawModel: string,
    provider: string | null,
    ignored: boolean,
  ) => {
    setSavingModel(true);
    setModelNotice(null);
    try {
      await setModelPricingIgnored(rawModel, provider, ignored);
      setModelNotice(
        ignored
          ? `已将【${rawModel}】标记为忽略（不计费），已重算历史用量`
          : `已恢复【${rawModel}】参与计价匹配`,
      );
      const [cat, st] = await Promise.all([
        getModelPricingCatalog(),
        getModelPricingStatus(),
      ]);
      setCatalog(cat);
      setStatus(st);
    } catch (err) {
      setModelNotice(err instanceof Error ? err.message : String(err));
      throw err;
    } finally {
      setSavingModel(false);
    }
  };

  if (loading && !catalog) {
    return (
      <div className="py-8">
        <AppSkeleton layout="cards" label="正在装载模型价格与汇率参数..." />
      </div>
    );
  }

  const localCount = catalog?.localModels
    ? catalog.localModels.length
    : (status?.localModelsCount ?? 0);
  const totalCount = catalog?.models
    ? catalog.models.length
    : (status?.modelsCount ?? 0);

  return (
    <div className="flex flex-col gap-6 w-full">
      {/* 1. 汇率管理与在线同步卡片 */}
      <Panel variant="default" className="p-5 rounded-2xl flex flex-col gap-4">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
          <div className="flex items-center gap-2.5">
            <span className="grid size-8 place-items-center rounded-xl border border-theme-control-border bg-theme-control text-primary">
              <Coins className="h-4 w-4" />
            </span>
            <div>
              <h3 className="text-body-md font-semibold text-on-surface">
                参考汇率与在线同步
              </h3>
              <p className="text-caption text-on-surface-variant">
                设置美元对人民币基准折算率，或从云端拉取全网模型最新参考价目。
              </p>
            </div>
          </div>
          {status && (
            <div className="flex flex-wrap items-center gap-2">
              <Badge
                tone="primary"
                className="rounded-full px-2.5 py-0.5 text-caption font-mono"
              >
                本地已用: {localCount} 款
              </Badge>
              <Badge
                tone="neutral"
                className="rounded-full px-2.5 py-0.5 text-caption font-mono"
              >
                全网库: {totalCount} 款
              </Badge>
              <Badge
                tone="neutral"
                className="rounded-full px-2.5 py-0.5 text-caption font-mono"
              >
                渠道: {status.activeChannel}
              </Badge>
            </div>
          )}
        </div>

        {/* 汇率修改与同步按钮行 */}
        <div className="flex flex-wrap items-center gap-3 pt-2">
          <div className="flex items-center gap-2 rounded-xl border border-theme-control-border bg-theme-control px-3 h-10">
            <span className="text-caption text-on-surface-variant font-medium">
              1 USD =
            </span>
            <Input
              type="number"
              step="0.01"
              value={rateInput}
              onChange={(e) => setRateInput(e.target.value)}
              className="w-20 h-7 border-none bg-transparent px-1 py-0 text-body-sm font-semibold text-on-surface focus-visible:ring-0"
            />
            <span className="text-caption text-on-surface-variant font-medium">
              CNY
            </span>
          </div>

          <Button
            variant="default"
            onClick={handleSaveRate}
            disabled={savingRate}
            className="h-10 rounded-xl px-4 text-body-sm shadow-[var(--theme-glow)]"
          >
            {savingRate ? (
              <RefreshCw className="mr-1.5 h-4 w-4 animate-spin" />
            ) : (
              <Check className="mr-1.5 h-4 w-4" />
            )}
            保存汇率并重算
          </Button>

          <Button
            variant="outline"
            onClick={handleTriggerSync}
            disabled={syncing}
            className="h-10 rounded-xl px-4 text-body-sm ml-auto"
          >
            <RefreshCw
              className={`mr-1.5 h-4 w-4 ${syncing ? "animate-spin text-primary" : ""}`}
            />
            {syncing ? "正在拉取最新单价..." : "在线同步最新价格"}
          </Button>
        </div>

        {/* 汇率提示 */}
        {rateNotice && (
          <div
            className={`flex items-center gap-2 rounded-xl border p-3 text-caption ${
              rateNotice.type === "success"
                ? "border-primary/30 bg-primary/10 text-primary"
                : "border-destructive/30 bg-destructive/10 text-destructive"
            }`}
          >
            {rateNotice.type === "success" ? (
              <CheckCircle2 className="h-4 w-4 shrink-0" />
            ) : (
              <AlertCircle className="h-4 w-4 shrink-0" />
            )}
            <span>{rateNotice.text}</span>
          </div>
        )}

        {/* 同步提示 */}
        {syncNotice && (
          <div
            className={`flex items-center gap-2 rounded-xl border p-3 text-caption ${
              syncNotice.type === "success"
                ? "border-primary/30 bg-primary/10 text-primary"
                : "border-destructive/30 bg-destructive/10 text-destructive"
            }`}
          >
            {syncNotice.type === "success" ? (
              <CheckCircle2 className="h-4 w-4 shrink-0" />
            ) : (
              <AlertCircle className="h-4 w-4 shrink-0" />
            )}
            <span>{syncNotice.text}</span>
          </div>
        )}
      </Panel>

      {/* 2. 本地已用模型资费管理（默认只展示本地已用模型） */}
      <ModelPricingTable
        localModels={catalog?.localModels || []}
        allModels={catalog?.models || []}
        onSaveModel={handleSaveModel}
        onRebindModel={handleRebindModel}
        onSetPriceLock={handleSetPriceLock}
        onSetModelIgnored={handleSetModelIgnored}
        savingModel={savingModel}
        notice={modelNotice}
      />
    </div>
  );
};

export default ConversationPricingSettingsPanel;
