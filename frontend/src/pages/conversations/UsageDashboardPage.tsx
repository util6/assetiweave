import React, { useState, useEffect, useCallback, useMemo, useRef } from "react";
import type {
  UsageDashboardDto,
  UsageDashboardFilter,
  UsageScanMode,
  UsageScanStatus,
} from "../../types/usage";
import type { TaskView } from "../../types/taskCenter";
import {
  getConversationUsageDashboard,
  getConversationUsageScanStatus,
  scanConversationUsage,
} from "../../services/usage";
import { UsageHeader } from "../../components/conversations/usage/UsageHeader";
import {
  UsageToolbar,
  type TimeRangeValue,
} from "../../components/conversations/usage/UsageToolbar";
import { UsageHeroCard } from "../../components/conversations/usage/UsageHeroCard";
import { UsageTrendChart } from "../../components/conversations/usage/UsageTrendChart";
import { UsageBreakdownTables } from "../../components/conversations/usage/UsageBreakdownTables";
import {
  UsageScanResultBanner,
  buildUsageScanNoticeFromTask,
  type UsageScanNotice,
} from "../../components/conversations/usage/UsageScanResultBanner";
import { AppSkeleton } from "../../components/foundation/skeleton";
import { AlertCircle } from "lucide-react";
import { useAppUiStore } from "../../store/ui/appUiStore";
import { useOptionalTaskCenter } from "../../app/backgroundTasks/TaskCenterProvider";
import type { AppShortcut } from "../../types";

interface UsageDashboardPageProps {
  appShortcuts?: AppShortcut[];
}

const USAGE_SCAN_KIND = "conversationusagescan";
const USAGE_SCAN_CATEGORY = "conversation/usage_scan";

function isUsageScanTask(task: TaskView): boolean {
  return task.kind === USAGE_SCAN_KIND || task.category === USAGE_SCAN_CATEGORY;
}

function isActiveScanState(state: TaskView["state"]): boolean {
  return state === "pending" || state === "running" || state === "cancelling";
}

export const UsageDashboardPage: React.FC<UsageDashboardPageProps> = ({
  appShortcuts = [],
}) => {
  const [timeRange, setTimeRange] = useState<TimeRangeValue>("last_30_days");
  const [startDate, setStartDate] = useState<string | null>(null);
  const [endDate, setEndDate] = useState<string | null>(null);
  const [selectedInstance, setSelectedInstance] = useState<string>("all");
  const [selectedProject, setSelectedProject] = useState<string>("all");
  const [dashboard, setDashboard] = useState<UsageDashboardDto | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [refreshing, setRefreshing] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [scanMode, setScanMode] = useState<UsageScanMode>("incremental");
  const [scanNotice, setScanNotice] = useState<UsageScanNotice | null>(null);
  const [activeScanTaskId, setActiveScanTaskId] = useState<string | null>(null);
  const [scanStatus, setScanStatus] = useState<UsageScanStatus>({
    scannedSourcesCount: 0,
    totalEventsCount: 0,
    lastScannedAt: null,
    activeScanTaskId: null,
    sourceDiagnostics: [],
  });

  const taskCenter = useOptionalTaskCenter();
  // 本页面发起或接管的扫描任务 id，用于区分历史终态任务与本次会话的扫描结果。
  const watchedTaskIdsRef = useRef<Set<string>>(new Set());
  const finalizedTaskIdsRef = useRef<Set<string>>(new Set());
  const taskCenterRef = useRef(taskCenter);
  useEffect(() => {
    taskCenterRef.current = taskCenter;
  }, [taskCenter]);

  const loadDashboard = useCallback(
    async (isBackground = false) => {
      if (!isBackground) setLoading(true);
      else setRefreshing(true);
      setError(null);
      try {
        const filter: UsageDashboardFilter = {
          timeRange,
          startDate,
          endDate,
          sourceId: selectedInstance === "all" ? undefined : selectedInstance,
          projectPath: selectedProject === "all" ? undefined : selectedProject,
          timezoneOffsetMinutes: -new Date().getTimezoneOffset(),
        };
        const data = await getConversationUsageDashboard(filter);
        setDashboard(data);
        if (data.scanStatus) {
          setScanStatus((prev) => ({
            ...prev,
            ...data.scanStatus,
          }));
        }
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setLoading(false);
        setRefreshing(false);
      }
    },
    [timeRange, startDate, endDate, selectedInstance, selectedProject],
  );

  const checkScanStatus = useCallback(async () => {
    try {
      const status = await getConversationUsageScanStatus();
      setScanStatus((prev) => ({
        ...prev,
        ...status,
      }));
      return status;
    } catch {
      return null;
    }
  }, []);

  const scanning = Boolean(activeScanTaskId || scanStatus.activeScanTaskId);

  const finalizeScan = useCallback(
    (taskId: string, notice: UsageScanNotice) => {
      if (finalizedTaskIdsRef.current.has(taskId)) return;
      finalizedTaskIdsRef.current.add(taskId);
      watchedTaskIdsRef.current.delete(taskId);
      setScanNotice(notice);
      setActiveScanTaskId((current) => (current === taskId ? null : current));
      void checkScanStatus();
      void loadDashboard(true);
    },
    [checkScanStatus, loadDashboard],
  );

  // Initial load and auto-scan if no events scanned yet
  useEffect(() => {
    let cancelled = false;
    async function init() {
      await loadDashboard();
      const status = await checkScanStatus();
      if (cancelled) return;
      if (
        status &&
        !status.activeScanTaskId &&
        (!status.lastScannedAt || status.totalEventsCount === 0)
      ) {
        try {
          const started = await scanConversationUsage({ mode: "incremental" });
          if (!cancelled && started.id) {
            watchedTaskIdsRef.current.add(started.id);
            setActiveScanTaskId(started.id);
            void taskCenterRef.current?.refresh();
          }
        } catch {
          // ignore background auto-scan startup error
        }
      }
    }
    void init();
    return () => {
      cancelled = true;
    };
  }, [loadDashboard, checkScanStatus]);

  // When user returns from settings (e.g. pricing settings), auto-refresh dashboard with updated costs
  const settingsPanel = useAppUiStore((state) => state.settingsPanel);
  const prevSettingsPanelRef = React.useRef(settingsPanel);
  useEffect(() => {
    if (prevSettingsPanelRef.current && !settingsPanel) {
      void loadDashboard(true);
    }
    prevSettingsPanelRef.current = settingsPanel;
  }, [settingsPanel, loadDashboard]);

  // 任务中心是运行态与终态结果的第一事实源：接管运行中任务、收敛终态文案。
  const usageScanTasks = useMemo(
    () => (taskCenter?.tasks ?? []).filter(isUsageScanTask),
    [taskCenter],
  );

  useEffect(() => {
    if (!taskCenter) return;

    const running = usageScanTasks.find((task) => isActiveScanState(task.state));
    if (running) {
      watchedTaskIdsRef.current.add(running.id);
      if (activeScanTaskId !== running.id) setActiveScanTaskId(running.id);
      return;
    }

    for (const task of usageScanTasks) {
      if (!watchedTaskIdsRef.current.has(task.id)) continue;
      if (finalizedTaskIdsRef.current.has(task.id)) continue;
      if (isActiveScanState(task.state)) continue;
      const notice = buildUsageScanNoticeFromTask(task);
      if (notice) finalizeScan(task.id, notice);
    }
  }, [taskCenter, usageScanTasks, activeScanTaskId, finalizeScan]);

  // 轮询兜底：任务中心不可用或无对应任务快照时，按扫描状态端点收敛运行态。
  useEffect(() => {
    const taskId = activeScanTaskId ?? scanStatus.activeScanTaskId;
    if (!taskId) return;
    const interval = setInterval(async () => {
      const status = await checkScanStatus();
      if (!status || status.activeScanTaskId) return;
      const taskCenterKnowsTask =
        Boolean(taskCenter) &&
        usageScanTasks.some(
          (task) => task.id === taskId || watchedTaskIdsRef.current.has(task.id),
        );
      if (taskCenterKnowsTask) return;
      finalizeScan(taskId, {
        tone: "success",
        text: "扫描完成，已刷新用量数据",
      });
    }, 2000);
    return () => clearInterval(interval);
  }, [
    activeScanTaskId,
    scanStatus.activeScanTaskId,
    checkScanStatus,
    taskCenter,
    usageScanTasks,
    finalizeScan,
  ]);

  const handleScan = async () => {
    setScanNotice(null);
    try {
      const started = await scanConversationUsage({
        mode: scanMode,
        sourceId: selectedInstance === "all" ? undefined : selectedInstance,
      });
      if (started.id) {
        watchedTaskIdsRef.current.add(started.id);
        setActiveScanTaskId(started.id);
        void taskCenter?.refresh();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <div className="flex flex-1 flex-col gap-[var(--app-section-gap)] px-[var(--app-page-x)] py-[var(--app-page-y)] pb-12 w-full">
      {/* Header with Title and Back button */}
      <UsageHeader />

      {/* Error notification banner */}
      {error && (
        <div className="flex items-center gap-2 rounded-xl border border-destructive/30 bg-destructive/10 p-4 text-body-sm text-destructive">
          <AlertCircle className="h-4 w-4 shrink-0" />
          <span>{error}</span>
        </div>
      )}

      {/* Toolbar: Time Range, Instance select, Project select, Refresh, Scan, Metadata line */}
      <UsageToolbar
        timeRange={timeRange}
        onTimeRangeChange={(r) => setTimeRange(r)}
        startDate={startDate}
        endDate={endDate}
        onDateRangeChange={(start, end) => {
          setStartDate(start);
          setEndDate(end);
        }}
        selectedInstance={selectedInstance}
        onInstanceChange={(inst) => setSelectedInstance(inst)}
        selectedProject={selectedProject}
        onProjectChange={(proj) => setSelectedProject(proj)}
        projects={dashboard?.projects || []}
        sources={dashboard?.sources || []}
        onRefresh={() => void loadDashboard(true)}
        onScan={() => void handleScan()}
        scanMode={scanMode}
        onScanModeChange={(mode) => setScanMode(mode)}
        scanning={scanning}
        scanStatus={scanStatus}
        totalRequests={dashboard?.hero.requestsCount || 0}
        refreshing={refreshing}
      />

      <UsageScanResultBanner
        notice={scanNotice}
        onDismiss={() => setScanNotice(null)}
      />

      {loading && !dashboard ? (
        <div className="py-12">
          <AppSkeleton layout="cards" label="正在统计用量数据..." />
        </div>
      ) : dashboard ? (
        <>
          {/* Summary Hero Cards: Left Big Card + Right 3 Sub-Cards */}
          <UsageHeroCard hero={dashboard.hero} />

          {/* Daily Trend Spline Area Chart with peak badges */}
          <UsageTrendChart data={dashboard.dailyTrend} />

          {/* Side-by-side Tables (By Model & By Instance) + Projects Table + Full-width By Date Table */}
          <UsageBreakdownTables
            models={dashboard.models}
            sources={dashboard.sources}
            projects={dashboard.projects}
            dates={dashboard.dates}
            selectedProject={selectedProject}
            onSelectProject={(proj) => setSelectedProject(proj)}
            appShortcuts={appShortcuts}
          />
        </>
      ) : null}
    </div>
  );
};

export default UsageDashboardPage;
