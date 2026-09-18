import React, { useEffect, useState } from "react";
import type {
  UsageScanStatus,
  UsageSourceBreakdown,
} from "../../../types/usage";
import { Button } from "../../ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "../../ui/select";
import { RefreshCw, RotateCw, AlertTriangle } from "lucide-react";
import { cn } from "../../../lib/utils";

export type TimeRangeValue =
  "last_30_days" | "today" | "last_7_days" | "last_90_days" | "all_time";

interface UsageToolbarProps {
  timeRange: TimeRangeValue;
  onTimeRangeChange: (range: TimeRangeValue) => void;
  selectedInstance: string;
  onInstanceChange: (instance: string) => void;
  sources: UsageSourceBreakdown[];
  onRefresh: () => void;
  onScan: () => void;
  scanStatus: UsageScanStatus;
  totalRequests: number;
  refreshing: boolean;
  className?: string;
}

export const UsageToolbar: React.FC<UsageToolbarProps> = ({
  timeRange,
  onTimeRangeChange,
  selectedInstance,
  onInstanceChange,
  sources,
  onRefresh,
  onScan,
  scanStatus,
  totalRequests,
  refreshing,
  className,
}) => {
  const [cooldownSecs, setCooldownSecs] = useState<number>(0);

  useEffect(() => {
    if (cooldownSecs <= 0) return;
    const timer = setInterval(() => {
      setCooldownSecs((prev) => Math.max(0, prev - 1));
    }, 1000);
    return () => clearInterval(timer);
  }, [cooldownSecs]);

  const isScanning = Boolean(scanStatus.activeScanTaskId);

  const handleScanClick = () => {
    if (isScanning || cooldownSecs > 0) return;
    onScan();
    setCooldownSecs(30);
  };

  const lastUpdatedText = scanStatus.lastScannedAt
    ? new Date(scanStatus.lastScannedAt).toLocaleTimeString([], {
        hour: "2-digit",
        minute: "2-digit",
      })
    : "刚刚";

  return (
    <div className={cn("flex flex-col gap-3", className)}>
      {/* Top Filter Bar */}
      <div className="flex flex-wrap items-center justify-between gap-4 rounded-2xl border border-theme-card-border/80 bg-surface/75 p-3.5 shadow-[var(--theme-shadow-card)] backdrop-blur-xl">
        <div className="flex flex-wrap items-center gap-4">
          {/* Time range selector */}
          <div className="flex items-center gap-2">
            <span className="text-body-sm font-medium text-on-surface-variant shrink-0">
              相对范围
            </span>
            <div className="w-[120px]">
              <Select
                value={timeRange}
                onValueChange={(val) =>
                  onTimeRangeChange(val as TimeRangeValue)
                }
              >
                <SelectTrigger
                  size="sm"
                  className="h-8 rounded-xl bg-theme-control/40"
                >
                  <SelectValue placeholder="选择范围" />
                </SelectTrigger>
                <SelectContent className="rounded-xl">
                  <SelectItem value="last_30_days">近 30 天</SelectItem>
                  <SelectItem value="today">今天</SelectItem>
                  <SelectItem value="last_7_days">近 7 天</SelectItem>
                  <SelectItem value="last_90_days">近 90 天</SelectItem>
                  <SelectItem value="all_time">全部</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          {/* Instance selector */}
          <div className="flex items-center gap-2">
            <span className="text-body-sm font-medium text-on-surface-variant shrink-0">
              实例
            </span>
            <div className="w-[130px]">
              <Select
                value={selectedInstance}
                onValueChange={(val) => onInstanceChange(val)}
              >
                <SelectTrigger
                  size="sm"
                  className="h-8 rounded-xl bg-theme-control/40"
                >
                  <SelectValue placeholder="选择实例" />
                </SelectTrigger>
                <SelectContent className="rounded-xl">
                  <SelectItem value="all">全部实例</SelectItem>
                  {sources.map((s) => (
                    <SelectItem key={s.sourceId} value={s.sourceId}>
                      {s.sourceName || s.appName}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>
        </div>

        {/* Action buttons */}
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={onRefresh}
            disabled={refreshing}
            className="h-8 gap-1.5 rounded-xl px-3 text-caption cursor-pointer border-theme-border/70"
          >
            <RefreshCw
              className={cn(
                "h-3.5 w-3.5",
                refreshing && "animate-spin text-primary",
              )}
            />
            <span>刷新</span>
          </Button>

          <Button
            variant="outline"
            size="sm"
            onClick={handleScanClick}
            disabled={isScanning || cooldownSecs > 0}
            className="h-8 gap-1.5 rounded-xl px-3 text-caption cursor-pointer border-theme-border/70"
          >
            <RotateCw
              className={cn(
                "h-3.5 w-3.5",
                isScanning && "animate-spin text-primary",
              )}
            />
            <span>
              {isScanning
                ? "正在扫描..."
                : cooldownSecs > 0
                  ? `重新扫描 (${cooldownSecs}s)`
                  : "重新扫描"}
            </span>
          </Button>
        </div>
      </div>

      {/* Metadata summary line */}
      <div className="px-1 text-caption text-on-surface-variant">
        已扫描 {scanStatus.scannedSourcesCount || sources.length || 0}{" "}
        个本地来源，累计 {totalRequests.toLocaleString()} 次请求 · 上次更新{" "}
        {lastUpdatedText}
      </div>

      {/* Notice Banner */}
      <div className="flex items-center gap-2.5 rounded-xl border border-theme-border/50 bg-theme-control/30 px-3.5 py-2.5 text-caption text-on-surface-variant">
        <AlertTriangle className="h-4 w-4 shrink-0 text-status-conflict" />
        <span>
          本地多源用量统计：当前已接入并统计 Codex、Antigravity、Claude Code、OpenCode、ZCode 等本地会话数据。费用根据模型定价推算生成。
        </span>
      </div>
    </div>
  );
};
