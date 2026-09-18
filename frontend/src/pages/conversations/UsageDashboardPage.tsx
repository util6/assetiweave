import React, { useCallback, useEffect, useState } from "react";
import type {
  UsageDashboardDto,
  UsageDashboardFilter,
  UsageScanStatus,
} from "../../types/usage";
import {
  getConversationUsageDashboard,
  getConversationUsageScanStatus,
  scanConversationUsage,
  subscribeUsageScanTask,
} from "../../services/usage";
import { UsageHeader } from "../../components/conversations/usage/UsageHeader";
import {
  UsageToolbar,
  type TimeRangeValue,
} from "../../components/conversations/usage/UsageToolbar";
import { UsageHeroCard } from "../../components/conversations/usage/UsageHeroCard";
import { UsageTrendChart } from "../../components/conversations/usage/UsageTrendChart";
import { UsageBreakdownTables } from "../../components/conversations/usage/UsageBreakdownTables";
import { AppSkeleton } from "../../components/foundation/skeleton";
import { AlertCircle } from "lucide-react";

export const UsageDashboardPage: React.FC = () => {
  const [timeRange, setTimeRange] = useState<TimeRangeValue>("last_30_days");
  const [selectedInstance, setSelectedInstance] = useState<string>("all");
  const [dashboard, setDashboard] = useState<UsageDashboardDto | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [refreshing, setRefreshing] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [scanStatus, setScanStatus] = useState<UsageScanStatus>({
    scannedSourcesCount: 0,
    totalEventsCount: 0,
    lastScannedAt: null,
    activeScanTaskId: null,
    sourceDiagnostics: [],
  });

  const loadDashboard = useCallback(
    async (isBackground = false) => {
      if (!isBackground) setLoading(true);
      else setRefreshing(true);
      setError(null);
      try {
        const filter: UsageDashboardFilter = {
          timeRange,
          sourceId: selectedInstance === "all" ? undefined : selectedInstance,
          timezoneOffsetMinutes: -new Date().getTimezoneOffset(),
        };
        const data = await getConversationUsageDashboard(filter);
        setDashboard(data);
        if (data.scanStatus) {
          setScanStatus((prev) => ({
            ...prev,
            ...data.scanStatus,
            activeScanTaskId: prev.activeScanTaskId || data.scanStatus.activeScanTaskId,
          }));
        }
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setLoading(false);
        setRefreshing(false);
      }
    },
    [timeRange, selectedInstance],
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
          if (!cancelled && started) {
            setScanStatus(started);
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

  // Subscribe to real-time background scan task events
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    void subscribeUsageScanTask((snapshot) => {
      if (cancelled) return;
      const isRunning =
        snapshot.status === "running" ||
        snapshot.status === "pending" ||
        snapshot.status === "preparing";

      if (isRunning) {
        setScanStatus((prev) => ({
          ...prev,
          activeScanTaskId: snapshot.id,
        }));
      } else {
        // completed, failed, or cancelled
        setScanStatus((prev) => ({
          ...prev,
          activeScanTaskId: null,
          lastScannedAt: snapshot.finishedAt || new Date().toISOString(),
        }));
        void loadDashboard(true);
      }
    })
      .then((cleanup) => {
        if (cancelled) {
          cleanup();
        } else {
          unlisten = cleanup;
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [loadDashboard]);

  // Poll scan status fallback while scanning in background
  useEffect(() => {
    if (!scanStatus.activeScanTaskId) return;
    const interval = setInterval(async () => {
      const status = await checkScanStatus();
      if (status && !status.activeScanTaskId) {
        void loadDashboard(true);
      }
    }, 2000);
    return () => clearInterval(interval);
  }, [scanStatus.activeScanTaskId, checkScanStatus, loadDashboard]);

  const handleScan = async () => {
    try {
      const started = await scanConversationUsage({
        mode: "incremental",
        sourceId: selectedInstance === "all" ? undefined : selectedInstance,
      });
      setScanStatus(started);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <div className="flex flex-col gap-4 p-6 max-w-7xl mx-auto w-full">
      {/* Header with Title and Back button */}
      <UsageHeader />

      {/* Error notification banner */}
      {error && (
        <div className="flex items-center gap-2 rounded-xl border border-destructive/30 bg-destructive/10 p-4 text-body-sm text-destructive">
          <AlertCircle className="h-4 w-4 shrink-0" />
          <span>{error}</span>
        </div>
      )}

      {/* Toolbar: Time Range, Instance select, Refresh, Scan, Metadata line, Notice banner */}
      <UsageToolbar
        timeRange={timeRange}
        onTimeRangeChange={(r) => setTimeRange(r)}
        selectedInstance={selectedInstance}
        onInstanceChange={(inst) => setSelectedInstance(inst)}
        sources={dashboard?.sources || []}
        onRefresh={() => void loadDashboard(true)}
        onScan={handleScan}
        scanStatus={scanStatus}
        totalRequests={dashboard?.hero.requestsCount || 0}
        refreshing={refreshing}
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

          {/* Side-by-side Tables (By Model & By Instance) + Full-width By Date Table */}
          <UsageBreakdownTables
            models={dashboard.models}
            sources={dashboard.sources}
            dates={dashboard.dates}
          />
        </>
      ) : null}
    </div>
  );
};

export default UsageDashboardPage;
