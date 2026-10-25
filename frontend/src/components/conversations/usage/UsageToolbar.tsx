import React, { useState, useEffect } from "react";
import type {
  UsageProjectBreakdown,
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
import { DateRangePicker } from "./DateRangePicker";

export type TimeRangeValue =
  | "last_30_days"
  | "today"
  | "last_7_days"
  | "last_90_days"
  | "all_time"
  | "custom";

interface UsageToolbarProps {
  timeRange: TimeRangeValue;
  onTimeRangeChange: (range: TimeRangeValue) => void;
  startDate?: string | null;
  endDate?: string | null;
  onDateRangeChange?: (start: string | null, end: string | null) => void;
  selectedInstance: string;
  onInstanceChange: (instance: string) => void;
  selectedProject?: string;
  onProjectChange?: (project: string) => void;
  projects?: UsageProjectBreakdown[];
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
  startDate,
  endDate,
  onDateRangeChange,
  selectedInstance,
  onInstanceChange,
  selectedProject = "all",
  onProjectChange,
  projects = [],
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
      {/* Top Filter Bar — relative z-30 提升层叠上下文，避免内部 DateRangePicker 弹层被下方 backdrop-blur 卡片（DOM 靠后）遮盖 */}
      <div className="relative z-30 flex flex-wrap items-center justify-between gap-4 rounded-2xl border border-theme-card-border/75 bg-[linear-gradient(135deg,rgb(var(--theme-toolbar-bg)/0.82),rgb(var(--theme-card-header)/0.72))] px-4 py-3 shadow-[var(--theme-shadow-toolbar),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.16)] backdrop-blur-xl">
        <div className="flex flex-wrap items-center gap-4">
          {/* Time range selector */}
          <div className="flex items-center gap-2">
            <span className="text-body-sm font-medium text-on-surface-variant shrink-0">
              相对范围
            </span>
            <div className="w-[124px]">
              <Select
                value={timeRange}
                onValueChange={(val) => {
                  const nextVal = val as TimeRangeValue;
                  if (nextVal !== "custom" && onDateRangeChange) {
                    onDateRangeChange(null, null);
                  }
                  onTimeRangeChange(nextVal);
                }}
              >
                <SelectTrigger
                  size="sm"
                  className="h-9 rounded-xl border-theme-control-border/80 bg-theme-control/60 shadow-[var(--theme-shadow-control-inset)] hover:border-theme-nav-active-border/50"
                >
                  <SelectValue placeholder="选择范围" />
                </SelectTrigger>
                <SelectContent className="rounded-xl border-theme-card-border bg-theme-card-header/95 backdrop-blur-xl">
                  {timeRange === "custom" && (
                    <SelectItem value="custom">自定义区间</SelectItem>
                  )}
                  <SelectItem value="last_30_days">近 30 天</SelectItem>
                  <SelectItem value="today">今天</SelectItem>
                  <SelectItem value="last_7_days">近 7 天</SelectItem>
                  <SelectItem value="last_90_days">近 90 天</SelectItem>
                  <SelectItem value="all_time">全部</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          {/* Date range picker */}
          <div className="flex items-center gap-2">
            <span className="text-body-sm font-medium text-on-surface-variant shrink-0">
              日期区间
            </span>
            <DateRangePicker
              startDate={startDate}
              endDate={endDate}
              onChange={(start, end) => {
                if (onDateRangeChange) {
                  onDateRangeChange(start, end);
                }
                if (start && end) {
                  onTimeRangeChange("custom");
                } else if (!start && !end) {
                  onTimeRangeChange("last_30_days");
                }
              }}
            />
          </div>

          {/* Instance selector */}
          <div className="flex items-center gap-2">
            <span className="text-body-sm font-medium text-on-surface-variant shrink-0">
              实例
            </span>
            <div className="w-[136px]">
              <Select
                value={selectedInstance}
                onValueChange={(val) => onInstanceChange(val)}
              >
                <SelectTrigger
                  size="sm"
                  className="h-9 rounded-xl border-theme-control-border/80 bg-theme-control/60 shadow-[var(--theme-shadow-control-inset)] hover:border-theme-nav-active-border/50"
                >
                  <SelectValue placeholder="选择实例" />
                </SelectTrigger>
                <SelectContent className="rounded-xl border-theme-card-border bg-theme-card-header/95 backdrop-blur-xl">
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

          {/* Project selector */}
          <div className="flex items-center gap-2">
            <span className="text-body-sm font-medium text-on-surface-variant shrink-0">
              项目
            </span>
            <div className="w-[156px]">
              <Select
                value={selectedProject}
                onValueChange={(val) => onProjectChange?.(val)}
              >
                <SelectTrigger
                  size="sm"
                  className="h-9 rounded-xl border-theme-control-border/80 bg-theme-control/60 shadow-[var(--theme-shadow-control-inset)] hover:border-theme-nav-active-border/50"
                >
                  <SelectValue placeholder="选择项目" />
                </SelectTrigger>
                <SelectContent className="rounded-xl border-theme-card-border bg-theme-card-header/95 backdrop-blur-xl max-h-[300px]">
                  <SelectItem value="all">全部项目</SelectItem>
                  {projects.map((p) => (
                    <SelectItem key={p.projectPath} value={p.projectPath}>
                      {p.displayName}
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
            className="h-9 gap-1.5 rounded-xl border-theme-control-border/80 bg-theme-control/60 px-3 text-caption text-theme-control-fg shadow-[var(--theme-shadow-control-inset)] cursor-pointer transition-all hover:border-theme-nav-active-border/60 hover:text-on-surface"
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
            className="h-9 gap-1.5 rounded-xl border-theme-control-border/80 bg-theme-control/60 px-3 text-caption text-theme-control-fg shadow-[var(--theme-shadow-control-inset)] cursor-pointer transition-all hover:border-theme-nav-active-border/60 hover:text-on-surface"
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
      <div className="px-1 text-caption text-on-surface-variant flex flex-wrap items-center justify-between gap-2">
        <div>
          已扫描 {scanStatus.scannedSourcesCount || sources.length || 0}{" "}
          个本地来源，累计 {totalRequests.toLocaleString()} 次请求 · 上次更新{" "}
          {lastUpdatedText}
        </div>
      </div>

      {/* Notice Banner */}
      <div className="flex items-center gap-2.5 rounded-xl border border-theme-card-border/60 bg-[linear-gradient(145deg,rgb(var(--theme-card-header)/0.65),rgb(var(--theme-control-bg)/0.55))] px-3.5 py-2.5 text-caption text-on-surface-variant shadow-[var(--theme-shadow-control-inset)] backdrop-blur-md">
        <AlertTriangle className="h-4 w-4 shrink-0 text-status-conflict" />
        <span>
          本地多源用量统计：当前已接入并统计 Codex、Antigravity、Claude
          Code、OpenCode、ZCode 等本地会话数据。费用根据模型定价推算生成。
        </span>
      </div>
    </div>
  );
};
