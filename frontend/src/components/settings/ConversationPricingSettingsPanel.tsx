import React, { useState, useEffect, useCallback, useMemo, useRef } from "react";
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
  ModelPricingSyncResult,
} from "@/types/usage";
import {
  getModelPricingCatalog,
  getModelPricingStatus,
  getModelPricingSyncTask,
  rebindModelPricingAlias,
  saveModelPriceEntry,
  setModelPricingExchangeRate,
  setModelPricingIgnored,
  setModelPricingLock,
  updateModelPricingCatalog,
} from "@/services/usage";
import { isActiveTaskState, type TaskView } from "@/types/taskCenter";
import { useOptionalTaskCenter } from "@/app/backgroundTasks/TaskCenterProvider";
import { taskInvalidationRegistry } from "@/app/backgroundTasks/taskInvalidationRegistry";
import { ModelPricingTable } from "./ModelPricingTable";

const PRICING_SYNC_KIND = "modelpricingsync";
const PRICING_SYNC_CATEGORY = "conversation/pricing_sync";
const SYNC_POLL_INTERVAL_MS = 2000;

function isPricingSyncTask(task: TaskView): boolean {
  return (
    task.kind === PRICING_SYNC_KIND || task.category === PRICING_SYNC_CATEGORY
  );
}

/** 用任务中心的阶段视图描述当前进度，避免依赖任何私有进度事件。 */
function describeSyncProgress(task: TaskView): string {
  const total = task.stages.length;
  if (total === 0) return "正在同步模型价格...";
  const finished = task.stages.filter(
    (stage) =>
      stage.status === "succeeded" ||
      stage.status === "partial_success" ||
      stage.status === "skipped" ||
      stage.status === "failed" ||
      stage.status === "canceled",
  ).length;
  const current =
    task.stages.find((stage) => stage.status === "running") ??
    task.stages.find((stage) => stage.status === "pending");
  return `${finished}/${total} 阶段${current ? ` · ${current.name}` : ""}`;
}

function isDegraded(result: ModelPricingSyncResult | null): boolean {
  if (!result) return false;
  return (
    result.downgraded === true ||
    Boolean(result.warning) ||
    result.channelUsed === "cache" ||
    result.channelUsed === "bundled"
  );
}

type PanelNoticeTone = "success" | "error" | "info";

const noticeToneClass: Record<PanelNoticeTone, string> = {
  success: "border-primary/30 bg-primary/10 text-primary",
  info: "border-status-update/35 bg-status-update/10 text-status-update",
  error: "border-destructive/30 bg-destructive/10 text-destructive",
};

const PanelNotice: React.FC<{ notice: { type: PanelNoticeTone; text: string } }> = ({
  notice,
}) => (
  <div
    className={`flex items-center gap-2 rounded-xl border p-3 text-caption ${noticeToneClass[notice.type]}`}
    role="status"
  >
    {notice.type === "success" ? (
      <CheckCircle2 className="h-4 w-4 shrink-0" />
    ) : (
      <AlertCircle className="h-4 w-4 shrink-0" />
    )}
    <span>{notice.text}</span>
  </div>
);

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
    type: "success" | "error" | "info";
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

  const taskCenter = useOptionalTaskCenter();
  const [syncProgressText, setSyncProgressText] = useState<string | null>(null);
  const watchedTaskIdRef = useRef<string | null>(null);
  const finalizedTaskIdsRef = useRef<Set<string>>(new Set());

  const pricingSyncTasks = useMemo(
    () => (taskCenter?.tasks ?? []).filter(isPricingSyncTask),
    [taskCenter?.tasks],
  );

  const applyTerminalTask = useCallback(
    (task: TaskView) => {
      if (finalizedTaskIdsRef.current.has(task.id)) return;
      finalizedTaskIdsRef.current.add(task.id);
      watchedTaskIdRef.current = null;
      setSyncing(false);
      setSyncProgressText(null);

      if (task.state === "succeeded") {
        const detail = (task.result ?? null) as ModelPricingSyncResult | null;
        setSyncNotice({
          type: isDegraded(detail) ? "info" : "success",
          text: detail
            ? `价格同步完成：渠道 ${detail.channelUsed}，更新 ${detail.modelsCount} 款模型，重算 ${detail.recalculatedEventsCount} 条会话用量（汇率 1 USD = ¥${detail.exchangeRate.toFixed(2)}）${detail.warning ? ` · ${detail.warning}` : ""}`
            : "价格同步完成，已重算历史用量",
        });
        void loadData();
        return;
      }

      setSyncNotice({
        type: "error",
        text:
          task.errorSummary ||
          (task.state === "canceled"
            ? "价格同步已取消"
            : "网络同步模型价格失败"),
      });
    },
    [loadData],
  );

  // 面板挂载 / 任务快照变化时：恢复运行中的同步任务态（阶段进度来自任务中心）。
  useEffect(() => {
    const running = pricingSyncTasks.find((task) =>
      isActiveTaskState(task.state),
    );
    if (running) {
      watchedTaskIdRef.current = running.id;
      setSyncing(true);
      setSyncProgressText(describeSyncProgress(running));
      return;
    }

    const watchedId = watchedTaskIdRef.current;
    if (!watchedId) {
      setSyncing(false);
      setSyncProgressText(null);
      return;
    }
    const finished = pricingSyncTasks.find((task) => task.id === watchedId);
    if (!finished || isActiveTaskState(finished.state)) return;
    applyTerminalTask(finished);
  }, [pricingSyncTasks, applyTerminalTask]);

  // 首屏兜底：任务中心尚无快照时，直接查询后台同步任务态。
  useEffect(() => {
    if (taskCenter) return;
    let cancelled = false;
    void (async () => {
      const task = await getModelPricingSyncTask();
      if (cancelled || !task) return;
      if (isActiveTaskState(task.state)) {
        watchedTaskIdRef.current = task.id;
        setSyncing(true);
        setSyncProgressText(describeSyncProgress(task));
      } else if (watchedTaskIdRef.current === task.id) {
        applyTerminalTask(task);
      }
    })().catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [taskCenter, applyTerminalTask]);

  // 事件丢失兜底：运行期轮询任务态，避免界面卡在旧状态。
  useEffect(() => {
    if (!syncing) return;
    const interval = setInterval(async () => {
      try {
        const task = await getModelPricingSyncTask();
        if (!task) return;
        if (isActiveTaskState(task.state)) {
          setSyncProgressText(describeSyncProgress(task));
          return;
        }
        applyTerminalTask(task);
      } catch {
        // 轮询失败不打扰用户，等待下一次 tick 或事件恢复。
      }
    }, SYNC_POLL_INTERVAL_MS);
    return () => clearInterval(interval);
  }, [syncing, applyTerminalTask]);

  // 终态事件联动价格目录缓存失效（ADR-0017）。
  useEffect(
    () =>
      taskInvalidationRegistry.register(PRICING_SYNC_CATEGORY, async () => {
        await loadData();
      }),
    [loadData],
  );

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
      // 同步在任务中心托管执行，完成态由任务事件驱动，此处只接管启动态。
      const task = await updateModelPricingCatalog(true);
      watchedTaskIdRef.current = task.id;
      const running =
        task.state === "running" ||
        task.state === "pending" ||
        task.state === "cancelling";
      setSyncing(running);
      if (!running) setSyncProgressText(null);
    } catch (err) {
      setSyncNotice({
        type: "error",
        text: err instanceof Error ? err.message : String(err),
      });
      setSyncing(false);
    }
  };

  const runModelMutation = async (
    mutation: () => Promise<unknown>,
    successNotice: string,
  ) => {
    setSavingModel(true);
    setModelNotice(null);
    try {
      await mutation();
      setModelNotice(successNotice);
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

  // Save updated model entry (auto-locks base model)
  const handleSaveModel = async (updated: ModelPriceEntry) => {
    const targetName =
      updated.matchedModelId || updated.rawModel || updated.modelPatterns[0];
    await runModelMutation(
      () => saveModelPriceEntry(updated),
      `模型【${targetName}】单价已保存并锁定（🔒），已重算历史用量`,
    );
  };

  const handleRebindModel = async (
    rawModel: string,
    targetModelId: string | null,
  ) => {
    await runModelMutation(
      () => rebindModelPricingAlias(rawModel, targetModelId),
      targetModelId
        ? `已将【${rawModel}】手动改绑至底座模型【${targetModelId}】，并重算历史用量`
        : `已解除【${rawModel}】的手动改绑，恢复自动匹配`,
    );
  };

  const handleSetPriceLock = async (modelId: string, locked: boolean) => {
    await runModelMutation(
      () => setModelPricingLock(modelId, locked),
      locked
        ? `已锁定底座模型【${modelId}】单价`
        : `已解锁底座模型【${modelId}】，立即回落全网参考单价并重算历史用量`,
    );
  };

  const handleSetModelIgnored = async (
    rawModel: string,
    provider: string | null,
    ignored: boolean,
  ) => {
    await runModelMutation(
      () => setModelPricingIgnored(rawModel, provider, ignored),
      ignored
        ? `已将【${rawModel}】标记为忽略（不计费），已重算历史用量`
        : `已恢复【${rawModel}】参与计价匹配`,
    );
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

        {/* 同步进度：阶段态来自任务中心，不依赖私有进度事件 */}
        {syncing && syncProgressText ? (
          <div
            className="flex items-center gap-2 rounded-xl border border-primary/25 bg-primary/5 px-3 py-2 text-caption text-on-surface-variant"
            role="status"
          >
            <RefreshCw className="h-3.5 w-3.5 animate-spin text-primary" />
            <span>{syncProgressText}</span>
          </div>
        ) : null}

        {/* 汇率提示 */}
        {rateNotice ? <PanelNotice notice={rateNotice} /> : null}

        {/* 同步提示：降级/部分可用走 info 态，失败走错误态 */}
        {syncNotice ? <PanelNotice notice={syncNotice} /> : null}
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
