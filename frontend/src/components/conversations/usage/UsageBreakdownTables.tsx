import React, { useState, useEffect } from "react";
import type {
  UsageDateBreakdown,
  UsageModelBreakdown,
  UsageProjectBreakdown,
  UsageSourceBreakdown,
} from "../../../types/usage";
import { formatChineseTokens, formatUsageCosts } from "../../../types/usage";
import { cn } from "../../../lib/utils";
import {
  AppShortcutIcon,
  AppShortcutIconForShortcut,
  APP_SHORTCUT_ICON_FRAME_CLASS,
  appShortcutIconFrameStyle,
  resolveAppShortcutAccentColor,
} from "../../apps/AppShortcutIcon";
import type { AppShortcut } from "../../../types";
import { Button } from "../../ui/button";
import { ChevronLeft, ChevronRight, FolderGit2, FolderArchive } from "lucide-react";

export function formatInstanceDisplayName(name: string): string {
  if (!name) return "";
  return (
    name.replace(/[\s\-_(]*local\s+sessions?[\s\-_)]*$/i, "").trim() || name
  );
}

export function findMatchedAppShortcut(
  source: UsageSourceBreakdown,
  appShortcuts?: AppShortcut[],
): AppShortcut | null {
  if (!appShortcuts || appShortcuts.length === 0) return null;
  const targetKind = (source.adapterId || source.appName || "").toLowerCase();
  return (
    appShortcuts.find(
      (sc) =>
        sc.profileId === source.sourceId ||
        sc.profileName?.toLowerCase() === source.appName?.toLowerCase(),
    ) ??
    appShortcuts.find(
      (sc) =>
        sc.appKind?.toLowerCase() === targetKind ||
        sc.profileName?.toLowerCase().includes(targetKind),
    ) ??
    null
  );
}

interface UsageBreakdownTablesProps {
  models: UsageModelBreakdown[];
  sources: UsageSourceBreakdown[];
  projects?: UsageProjectBreakdown[];
  dates: UsageDateBreakdown[];
  selectedProject?: string;
  onSelectProject?: (projectPath: string) => void;
  appShortcuts?: AppShortcut[];
  className?: string;
}

const DATES_PAGE_SIZE = 10;
const PROJECTS_PAGE_SIZE = 10;

export const UsageBreakdownTables: React.FC<UsageBreakdownTablesProps> = ({
  models,
  sources,
  projects = [],
  dates,
  selectedProject,
  onSelectProject,
  appShortcuts = [],
  className,
}) => {
  const [datePage, setDatePage] = useState<number>(1);
  const [projectPage, setProjectPage] = useState<number>(1);

  // Reset to page 1 whenever dates dataset changes
  useEffect(() => {
    setDatePage(1);
  }, [dates]);

  // Reset to page 1 whenever projects dataset changes
  useEffect(() => {
    setProjectPage(1);
  }, [projects]);

  const totalDatePages = Math.max(1, Math.ceil(dates.length / DATES_PAGE_SIZE));
  const validDatePage = Math.min(Math.max(1, datePage), totalDatePages);
  const pagedDates = dates.slice(
    (validDatePage - 1) * DATES_PAGE_SIZE,
    validDatePage * DATES_PAGE_SIZE,
  );

  const totalProjectPages = Math.max(1, Math.ceil(projects.length / PROJECTS_PAGE_SIZE));
  const validProjectPage = Math.min(Math.max(1, projectPage), totalProjectPages);
  const pagedProjects = projects.slice(
    (validProjectPage - 1) * PROJECTS_PAGE_SIZE,
    validProjectPage * PROJECTS_PAGE_SIZE,
  );

  return (
    <div className={cn("flex flex-col gap-4", className)}>
      {/* Top Row: Side-by-side Tables for Models and Instances */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        {/* Table 1: By Model (按模型) */}
        <div className="flex flex-col rounded-2xl border border-theme-card-border/75 bg-[linear-gradient(145deg,rgb(var(--theme-card-bg)/0.82),rgb(var(--theme-card-header)/0.74))] p-5 sm:p-6 shadow-[var(--theme-shadow-card),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.14)] backdrop-blur-xl transition-all">
          <div className="mb-3.5 flex items-center justify-between">
            <h3 className="text-body-md font-bold text-on-surface">按模型</h3>
          </div>
          <div className="overflow-x-auto -mx-1 px-1">
            <table className="w-full text-left text-body-sm">
              <thead>
                <tr className="border-b border-theme-border/40 text-caption font-semibold text-on-surface-variant">
                  <th className="pb-2.5 pl-2 pr-3">模型</th>
                  <th className="pb-2.5 pr-3 text-right">输入 Tokens</th>
                  <th className="pb-2.5 pr-3 text-right">缓存 Tokens</th>
                  <th className="pb-2.5 pr-3 text-right">输出 Tokens</th>
                  <th className="pb-2.5 pr-3 text-right">参考单价</th>
                  <th className="pb-2.5 pr-2 text-right font-bold text-primary">预估费用</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-theme-border/20">
                {models.length === 0 ? (
                  <tr>
                    <td
                      colSpan={6}
                      className="py-6 text-center text-caption text-on-surface-variant"
                    >
                      暂无模型用量记录
                    </td>
                  </tr>
                ) : (
                  models.map((m) => (
                    <tr
                      key={`${m.provider}-${m.model}`}
                      className="hover:bg-theme-control/35 transition-colors"
                    >
                      <td className="py-2.5 pl-2 pr-3 font-mono text-caption text-on-surface truncate max-w-[160px]">
                        {m.model}
                      </td>
                      <td className="py-2.5 pr-3 text-right font-mono text-caption text-on-surface">
                        {formatChineseTokens(m.inputTokens)}
                      </td>
                      <td className="py-2.5 pr-3 text-right font-mono text-caption text-on-surface">
                        {formatChineseTokens(m.cacheTokens)}
                      </td>
                      <td className="py-2.5 pr-3 text-right font-mono text-caption text-on-surface">
                        {formatChineseTokens(m.outputTokens)}
                      </td>
                      <td className="py-2.5 pr-3 text-right font-mono text-caption text-on-surface-variant">
                        {typeof m.unitPricePerMillion === "number" &&
                        m.unitPricePerMillion > 0
                          ? `¥${m.unitPricePerMillion.toFixed(2)}/M`
                          : "-"}
                      </td>
                      <td className="py-2.5 pr-2 text-right font-mono text-caption font-bold text-primary">
                        {typeof m.cost === "number"
                          ? `¥${m.cost.toFixed(2)}`
                          : "-"}
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>

        {/* Table 2: By Instance (按实例) */}
        <div className="flex flex-col justify-between rounded-2xl border border-theme-card-border/75 bg-[linear-gradient(145deg,rgb(var(--theme-card-bg)/0.82),rgb(var(--theme-card-header)/0.74))] p-5 sm:p-6 shadow-[var(--theme-shadow-card),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.14)] backdrop-blur-xl transition-all">
          <div>
            <div className="mb-3.5 flex items-center justify-between">
              <div className="flex items-center gap-2.5">
                <h3 className="text-body-md font-bold text-on-surface">按实例</h3>
                {sources.length > 0 && (
                  <span className="rounded-full bg-theme-control/70 border border-theme-control-border/60 px-2 py-0.5 text-[11px] font-mono text-on-surface-variant">
                    {sources.length} 个实例
                  </span>
                )}
              </div>
            </div>
            <div className="overflow-x-auto -mx-1 px-1">
              <table className="w-full text-left text-body-sm">
                <thead>
                  <tr className="border-b border-theme-border/40 text-caption font-semibold text-on-surface-variant">
                    <th className="pb-3 pl-2 pr-3">实例</th>
                    <th className="pb-3 pr-3 text-right">输入 Tokens</th>
                    <th className="pb-3 pr-3 text-right">缓存 Tokens</th>
                    <th className="pb-3 pr-3 text-right">输出 Tokens</th>
                    <th className="pb-3 pr-3 text-right font-bold">合计 Tokens</th>
                    <th className="pb-3 pr-2 text-right font-bold text-primary">预估费用</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-theme-border/20">
                  {sources.length === 0 ? (
                    <tr>
                      <td
                        colSpan={6}
                        className="py-10 text-center text-caption text-on-surface-variant"
                      >
                        暂无实例用量记录
                      </td>
                    </tr>
                  ) : (
                    sources.map((s) => {
                      const matchedShortcut = findMatchedAppShortcut(
                        s,
                        appShortcuts,
                      );
                      const accentColor =
                        matchedShortcut?.accentColor ??
                        resolveAppShortcutAccentColor(
                          {
                            appKind: s.adapterId || s.appName,
                            profileId: s.sourceId,
                            profileName: s.appName || s.sourceName,
                          },
                          appShortcuts,
                        );
                      const frameStyle = appShortcutIconFrameStyle(accentColor);
                      const displayName = formatInstanceDisplayName(
                        s.sourceName || s.appName,
                      );

                      return (
                        <tr
                          key={s.sourceId}
                          className="hover:bg-theme-control/35 transition-colors group"
                        >
                          <td className="py-4.5 sm:py-5 pl-2 pr-3 align-middle">
                            <div className="flex items-center gap-3">
                              <span
                                aria-hidden="true"
                                className={cn(
                                  APP_SHORTCUT_ICON_FRAME_CLASS,
                                  "size-10 shrink-0 text-[13px] font-bold shadow-xs transition-transform duration-200 group-hover:scale-105",
                                )}
                                style={frameStyle}
                              >
                                {matchedShortcut ? (
                                  <AppShortcutIconForShortcut
                                    className="size-5"
                                    shortcut={matchedShortcut}
                                  />
                                ) : (
                                  <AppShortcutIcon
                                    appKind={s.adapterId || s.appName}
                                    displayIcon={s.appName || s.sourceName}
                                    className="size-5 shrink-0"
                                  />
                                )}
                              </span>
                              <div className="flex flex-col min-w-0">
                                <span
                                  className="font-bold text-body-sm text-on-surface truncate max-w-[140px]"
                                  title={s.sourceName || s.appName}
                                >
                                  {displayName}
                                </span>
                                <span className="font-mono text-[11px] text-on-surface-variant/70 truncate max-w-[140px]">
                                  {s.appName &&
                                  s.appName.toLowerCase() !==
                                    displayName.toLowerCase()
                                    ? s.appName
                                    : s.adapterId &&
                                        s.adapterId.toLowerCase() !==
                                          displayName.toLowerCase()
                                      ? s.adapterId
                                      : "本地会话"}
                                </span>
                              </div>
                            </div>
                          </td>
                          <td className="py-4.5 sm:py-5 pr-3 text-right font-mono text-body-sm font-semibold text-on-surface align-middle">
                            {formatChineseTokens(s.inputTokens)}
                          </td>
                          <td className="py-4.5 sm:py-5 pr-3 text-right font-mono text-body-sm font-semibold text-on-surface align-middle">
                            {formatChineseTokens(s.cacheTokens)}
                          </td>
                          <td className="py-4.5 sm:py-5 pr-3 text-right font-mono text-body-sm font-semibold text-on-surface align-middle">
                            {formatChineseTokens(s.outputTokens)}
                          </td>
                          <td className="py-4.5 sm:py-5 pr-3 text-right font-mono text-body-sm font-bold text-on-surface align-middle">
                            {formatChineseTokens(s.totalTokens)}
                          </td>
                          <td className="py-4.5 sm:py-5 pr-2 text-right font-mono text-body-md font-extrabold text-primary align-middle">
                            {formatUsageCosts(s.costsByCurrency)}
                          </td>
                        </tr>
                      );
                    })
                  )}
                </tbody>
              </table>
            </div>
          </div>

          {/* Bottom metadata banner to fill natural vertical space */}
          {sources.length > 0 && (
            <div className="mt-4 pt-3.5 border-t border-theme-border/20 flex flex-wrap items-center justify-between gap-2 text-caption text-on-surface-variant">
              <span className="flex items-center gap-1.5 font-medium">
                <span className="size-2 rounded-full bg-status-create animate-pulse" />
                <span>已连接 {sources.length} 个本地活跃会话来源</span>
              </span>
              <span className="font-mono text-[11px] text-on-surface-variant/65">
                同步自本地会话原始存储
              </span>
            </div>
          )}
        </div>
      </div>

      {/* Middle Row: Full-width Table for Projects (按项目) */}
      <div className="flex flex-col rounded-2xl border border-theme-card-border/75 bg-[linear-gradient(145deg,rgb(var(--theme-card-bg)/0.82),rgb(var(--theme-card-header)/0.74))] p-5 sm:p-6 shadow-[var(--theme-shadow-card),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.14)] backdrop-blur-xl transition-all">
        <div className="mb-3.5 flex items-center justify-between">
          <div className="flex items-center gap-2.5">
            <h3 className="text-body-md font-bold text-on-surface">按项目</h3>
            {projects.length > 0 && (
              <span className="rounded-full bg-theme-control/70 border border-theme-control-border/60 px-2 py-0.5 text-[11px] font-mono text-on-surface-variant">
                共 {projects.length} 个项目
              </span>
            )}
          </div>
          {selectedProject && selectedProject !== "all" && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => onSelectProject?.("all")}
              className="h-7 text-xs text-primary hover:text-primary/80 px-2 rounded-lg cursor-pointer"
            >
              清除项目过滤
            </Button>
          )}
        </div>
        <div className="overflow-x-auto -mx-1 px-1">
          <table className="w-full text-left text-body-sm">
            <thead>
              <tr className="border-b border-theme-border/40 text-caption font-semibold text-on-surface-variant">
                <th className="pb-2.5 pl-2 pr-4">项目</th>
                <th className="pb-2.5 pr-4 text-right">输入 Tokens</th>
                <th className="pb-2.5 pr-4 text-right">缓存 Tokens</th>
                <th className="pb-2.5 pr-4 text-right">输出 Tokens</th>
                <th className="pb-2.5 pr-4 text-right font-bold">
                  合计 Tokens
                </th>
                <th className="pb-2.5 pr-4 text-right font-bold text-primary">预估费用</th>
                <th className="pb-2.5 pr-2 text-right font-semibold text-on-surface-variant">请求数</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-theme-border/20">
              {projects.length === 0 ? (
                <tr>
                  <td
                    colSpan={7}
                    className="py-6 text-center text-caption text-on-surface-variant"
                  >
                    暂无项目用量记录
                  </td>
                </tr>
              ) : (
                pagedProjects.map((p) => {
                  const isSelected = selectedProject === p.projectPath;
                  return (
                    <tr
                      key={p.projectPath}
                      onClick={() => {
                        if (onSelectProject) {
                          onSelectProject(isSelected ? "all" : p.projectPath);
                        }
                      }}
                      className={cn(
                        "hover:bg-theme-control/35 transition-colors cursor-pointer group",
                        isSelected && "bg-primary/10 hover:bg-primary/15",
                      )}
                      title={isSelected ? "点击取消项目过滤" : "点击过滤该项目"}
                    >
                      <td className="py-2.5 pl-2 pr-4 font-mono text-caption text-on-surface">
                        <div className="flex items-center gap-2 max-w-[280px] sm:max-w-[360px]">
                          {p.isUnassigned ? (
                            <FolderArchive className="size-4 shrink-0 text-on-surface-variant/60" />
                          ) : (
                            <FolderGit2 className="size-4 shrink-0 text-primary/70 group-hover:text-primary transition-colors" />
                          )}
                          <div className="flex flex-col min-w-0">
                            <span
                              className={cn(
                                "text-caption font-semibold truncate",
                                isSelected ? "text-primary font-bold" : "text-on-surface",
                              )}
                            >
                              {p.displayName}
                            </span>
                            {!p.isUnassigned && p.projectPath !== p.displayName && (
                              <span
                                className="text-[11px] font-mono text-on-surface-variant/70 truncate"
                                title={p.projectPath}
                              >
                                {p.projectPath}
                              </span>
                            )}
                          </div>
                          {isSelected && (
                            <span className="ml-auto shrink-0 rounded-full bg-primary/20 text-primary text-[10px] px-1.5 py-0.5 font-medium">
                              已筛选
                            </span>
                          )}
                        </div>
                      </td>
                      <td className="py-2.5 pr-4 text-right font-mono text-caption text-on-surface">
                        {formatChineseTokens(p.inputTokens)}
                      </td>
                      <td className="py-2.5 pr-4 text-right font-mono text-caption text-on-surface">
                        {formatChineseTokens(p.cacheTokens)}
                      </td>
                      <td className="py-2.5 pr-4 text-right font-mono text-caption text-on-surface">
                        {formatChineseTokens(p.outputTokens)}
                      </td>
                      <td className="py-2.5 pr-4 text-right font-mono text-caption font-bold text-on-surface">
                        {formatChineseTokens(p.totalTokens)}
                      </td>
                      <td className="py-2.5 pr-4 text-right font-mono text-caption font-bold text-primary">
                        {formatUsageCosts(p.costsByCurrency)}
                      </td>
                      <td className="py-2.5 pr-2 text-right font-mono text-caption text-on-surface-variant">
                        {p.requestsCount.toLocaleString()}
                      </td>
                    </tr>
                  );
                })
              )}
            </tbody>
          </table>
        </div>

        {/* Pagination Bar for Projects Table */}
        {totalProjectPages > 1 && (
          <div className="mt-4 flex flex-wrap items-center justify-between gap-3 border-t border-theme-border/30 pt-3 text-caption text-on-surface-variant">
            <div className="flex items-center gap-1.5 font-medium">
              <span className="font-mono text-on-surface font-semibold">
                第 {validProjectPage} / {totalProjectPages} 页
              </span>
              <span className="text-on-surface-variant/50">·</span>
              <span>共 {projects.length} 个项目</span>
            </div>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => setProjectPage((p) => Math.max(1, p - 1))}
                disabled={validProjectPage <= 1}
                className="h-8 gap-1 rounded-xl border-theme-control-border/80 bg-theme-control/60 px-2.5 text-caption text-theme-control-fg shadow-[var(--theme-shadow-control-inset)] transition-all hover:border-theme-nav-active-border/60 hover:text-on-surface disabled:opacity-40 cursor-pointer"
              >
                <ChevronLeft className="size-3.5" />
                <span>上一页</span>
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => setProjectPage((p) => Math.min(totalProjectPages, p + 1))}
                disabled={validProjectPage >= totalProjectPages}
                className="h-8 gap-1 rounded-xl border-theme-control-border/80 bg-theme-control/60 px-2.5 text-caption text-theme-control-fg shadow-[var(--theme-shadow-control-inset)] transition-all hover:border-theme-nav-active-border/60 hover:text-on-surface disabled:opacity-40 cursor-pointer"
              >
                <span>下一页</span>
                <ChevronRight className="size-3.5" />
              </Button>
            </div>
          </div>
        )}
      </div>

      {/* Bottom Row: Full-width Table for Dates (按日期) */}
      <div className="flex flex-col rounded-2xl border border-theme-card-border/75 bg-[linear-gradient(145deg,rgb(var(--theme-card-bg)/0.82),rgb(var(--theme-card-header)/0.74))] p-5 sm:p-6 shadow-[var(--theme-shadow-card),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.14)] backdrop-blur-xl transition-all">
        <div className="mb-3.5 flex items-center justify-between">
          <div className="flex items-center gap-2.5">
            <h3 className="text-body-md font-bold text-on-surface">按日期</h3>
            {dates.length > 0 && (
              <span className="rounded-full bg-theme-control/70 border border-theme-control-border/60 px-2 py-0.5 text-[11px] font-mono text-on-surface-variant">
                共 {dates.length} 天
              </span>
            )}
          </div>
        </div>
        <div className="overflow-x-auto -mx-1 px-1">
          <table className="w-full text-left text-body-sm">
            <thead>
              <tr className="border-b border-theme-border/40 text-caption font-semibold text-on-surface-variant">
                <th className="pb-2.5 pl-2 pr-4">日期</th>
                <th className="pb-2.5 pr-4 text-right">输入 Tokens</th>
                <th className="pb-2.5 pr-4 text-right">缓存 Tokens</th>
                <th className="pb-2.5 pr-4 text-right">输出 Tokens</th>
                <th className="pb-2.5 pr-4 text-right font-bold">
                  合计 Tokens
                </th>
                <th className="pb-2.5 pr-4 text-right font-bold text-primary">预估费用</th>
                <th className="pb-2.5 pr-2 text-right font-semibold text-on-surface-variant">请求数</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-theme-border/20">
              {dates.length === 0 ? (
                <tr>
                  <td
                    colSpan={7}
                    className="py-6 text-center text-caption text-on-surface-variant"
                  >
                    暂无每日用量记录
                  </td>
                </tr>
              ) : (
                pagedDates.map((d) => (
                  <tr
                    key={d.date}
                    className="hover:bg-theme-control/35 transition-colors"
                  >
                    <td className="py-2.5 pl-2 pr-4 font-mono text-caption text-on-surface">
                      {d.date}
                    </td>
                    <td className="py-2.5 pr-4 text-right font-mono text-caption text-on-surface">
                      {formatChineseTokens(d.inputTokens)}
                    </td>
                    <td className="py-2.5 pr-4 text-right font-mono text-caption text-on-surface">
                      {formatChineseTokens(d.cacheTokens)}
                    </td>
                    <td className="py-2.5 pr-4 text-right font-mono text-caption text-on-surface">
                      {formatChineseTokens(d.outputTokens)}
                    </td>
                    <td className="py-2.5 pr-4 text-right font-mono text-caption font-bold text-on-surface">
                      {formatChineseTokens(d.totalTokens)}
                    </td>
                    <td className="py-2.5 pr-4 text-right font-mono text-caption font-bold text-primary">
                      {formatUsageCosts(d.costsByCurrency)}
                    </td>
                    <td className="py-2.5 pr-2 text-right font-mono text-caption text-on-surface-variant">
                      {d.requestsCount.toLocaleString()}
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>

        {/* Pagination Bar for Dates Table */}
        {totalDatePages > 1 && (
          <div className="mt-4 flex flex-wrap items-center justify-between gap-3 border-t border-theme-border/30 pt-3 text-caption text-on-surface-variant">
            <div className="flex items-center gap-1.5 font-medium">
              <span className="font-mono text-on-surface font-semibold">
                第 {validDatePage} / {totalDatePages} 页
              </span>
              <span className="text-on-surface-variant/50">·</span>
              <span>共 {dates.length} 条记录</span>
            </div>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => setDatePage((p) => Math.max(1, p - 1))}
                disabled={validDatePage <= 1}
                className="h-8 gap-1 rounded-xl border-theme-control-border/80 bg-theme-control/60 px-2.5 text-caption text-theme-control-fg shadow-[var(--theme-shadow-control-inset)] transition-all hover:border-theme-nav-active-border/60 hover:text-on-surface disabled:opacity-40"
              >
                <ChevronLeft className="size-3.5" />
                <span>上一页</span>
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => setDatePage((p) => Math.min(totalDatePages, p + 1))}
                disabled={validDatePage >= totalDatePages}
                className="h-8 gap-1 rounded-xl border-theme-control-border/80 bg-theme-control/60 px-2.5 text-caption text-theme-control-fg shadow-[var(--theme-shadow-control-inset)] transition-all hover:border-theme-nav-active-border/60 hover:text-on-surface disabled:opacity-40"
              >
                <span>下一页</span>
                <ChevronRight className="size-3.5" />
              </Button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
