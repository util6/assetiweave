import {
  AlertCircle,
  Calendar,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Clock3,
  Copy,
  ExternalLink,
  FolderOpen,
  Sparkles,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import clsx from "clsx";
import type { Translator } from "../../i18n/I18nProvider";
import {
  getRecentMemorySnapshot,
  subscribeMemoryTasks,
} from "../../services/memory";
import type {
  MemoryNavigationTarget,
  RecentMemoryEvent,
  RecentMemoryItemView,
  RecentMemorySnapshotView,
  RecentMemoryStateView,
  RecentProjectView,
  RecentSessionReferenceView,
} from "../../types/memory";
import { PillTabs, type PillTabItem } from "../common/PillTabs";
import { EmptyState } from "../foundation/EmptyState";
import { Panel } from "../foundation/Panel";
import { AppSkeleton } from "../foundation/skeleton";
import { MarkdownContent } from "../conversations/ConversationMarkdown";

export interface MemoryRecentWorkspaceProps {
  onEventOpen?: (event: RecentMemoryEvent) => void;
  onNavigateSession?: (target: MemoryNavigationTarget) => void;
  t: Translator;
}

export function MemoryRecentWorkspace({
  onEventOpen: _legacyOnEventOpen,
  onNavigateSession,
  t,
}: MemoryRecentWorkspaceProps) {
  const [view, setView] = useState<"time" | "project">("time");
  const [stateView, setStateView] = useState<RecentMemoryStateView | null>(null);
  const [loading, setLoading] = useState(true);
  const [expandedItemIds, setExpandedItemIds] = useState<Set<string>>(new Set());

  const toggleItem = useCallback((itemId: string) => {
    setExpandedItemIds((prev) => {
      const next = new Set(prev);
      if (next.has(itemId)) {
        next.delete(itemId);
      } else {
        next.add(itemId);
      }
      return next;
    });
  }, []);

  const loadSnapshot = useCallback(async () => {
    try {
      const res = await getRecentMemorySnapshot();
      setStateView(res);
    } catch {
      // 容错处理，不弹阻断性弹窗 (M35-UI-06)
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadSnapshot();
    const unsubPromise = subscribeMemoryTasks(() => {
      void loadSnapshot();
    });
    return () => {
      void unsubPromise.then((unsub) => unsub?.());
    };
  }, [loadSnapshot]);

  const snapshot = stateView?.snapshot ?? null;

  // 客户端时间视图分组 (M35-UI-01, M35-UI-02)
  const timeViewData = useMemo(() => {
    if (!snapshot) return [];

    const dateMap = new Map<
      string,
      Map<
        string,
        {
          project: RecentProjectView;
          showWindowSummaryAndSuggestions: boolean;
          items: RecentMemoryItemView[];
        }
      >
    >();

    for (const proj of snapshot.projects) {
      const projSummaryDate = proj.latestActivityAt.split("T")[0] || "unknown";

      // 确保项目的摘要归入 latestActivityAt 当天
      if (!dateMap.has(projSummaryDate)) {
        dateMap.set(projSummaryDate, new Map());
      }
      const projMap = dateMap.get(projSummaryDate)!;
      if (!projMap.has(proj.projectKey)) {
        projMap.set(proj.projectKey, {
          project: proj,
          showWindowSummaryAndSuggestions: true,
          items: [],
        });
      } else {
        projMap.get(proj.projectKey)!.showWindowSummaryAndSuggestions = true;
      }

      // 普通条目归入 occurredAt 当天
      for (const item of proj.items) {
        const itemDate = item.occurredAt.split("T")[0] || "unknown";
        if (!dateMap.has(itemDate)) {
          dateMap.set(itemDate, new Map());
        }
        const m = dateMap.get(itemDate)!;
        if (!m.has(proj.projectKey)) {
          m.set(proj.projectKey, {
            project: proj,
            showWindowSummaryAndSuggestions: false,
            items: [item],
          });
        } else {
          m.get(proj.projectKey)!.items.push(item);
        }
      }
    }

    // 日期降序排序
    const sortedDates = [...dateMap.keys()].sort((a, b) => b.localeCompare(a));

    return sortedDates.map((date) => {
      const projsInDate = [...dateMap.get(date)!.values()].sort((a, b) => {
        const titleCmp = a.project.projectTitle.localeCompare(b.project.projectTitle);
        if (titleCmp !== 0) return titleCmp;
        return a.project.projectKey.localeCompare(b.project.projectKey);
      });

      return {
        date,
        projectGroups: projsInDate,
      };
    });
  }, [snapshot]);

  // 客户端项目视图分组 (M35-UI-01, M35-UI-02)
  const projectViewData = useMemo(() => {
    if (!snapshot) return [];

    return [...snapshot.projects]
      .sort((a, b) => b.latestActivityAt.localeCompare(a.latestActivityAt))
      .map((proj) => ({
        project: proj,
        items: [...proj.items].sort((a, b) => b.occurredAt.localeCompare(a.occurredAt)),
      }));
  }, [snapshot]);

  const pillItems: PillTabItem<"time" | "project">[] = useMemo(
    () => [
      {
        id: "time",
        label: t("memory.recent.timeView"),
        icon: <Clock3 size={14} />,
      },
      {
        id: "project",
        label: t("memory.recent.projectView"),
        icon: <FolderOpen size={14} />,
      },
    ],
    [t],
  );

  if (loading && !stateView) {
    return <AppSkeleton label={t("common.loading")} layout="list" />;
  }

  if (!snapshot && stateView?.status === "generating") {
    return <AppSkeleton label={t("memory.recent.generating")} layout="list" />;
  }

  const todayStr = new Date().toISOString().split("T")[0];
  const windowInfo = snapshot
    ? `${snapshot.windowHours}h · ${formatWatermark(snapshot.targetWatermark)}`
    : `48h · ${formatWatermark(new Date().toISOString())}`;

  if (!snapshot || snapshot.projects.length === 0) {
    return (
      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-hidden">
        {/* 顶部控制栏 (M35-UI-05: 仅投影切换与状态展示，DOM 绝对无刷新/生成/窗口输入) */}
        <div className="flex shrink-0 items-center justify-between gap-3 px-1">
          <div className="flex items-center gap-3">
            <PillTabs<"time" | "project">
              activeId={view}
              items={pillItems}
              onSelect={(id) => setView(id)}
              size="sm"
            />
            <span className="text-caption font-medium text-on-surface-variant">
              {windowInfo}
            </span>
          </div>

          <div>{renderStatusBadge(stateView, t)}</div>
        </div>

        {/* 视图内容区 (空状态手账 Date Rail 导轨) */}
        <div className="min-h-0 flex-1 overflow-auto pr-2 pb-8">
          {view === "time" ? (
            <div className="flex flex-col gap-8">
              {(() => {
                const dateParts = parseDateRailParts(todayStr, undefined, t);
                return (
                  <section className="relative min-w-0 pl-[84px] sm:pl-[98px] mt-2">
                    {/* 左侧垂直时间轴导轨 Date Rail */}
                    <div
                      aria-label={`${todayStr}, ${dateParts.relative}`}
                      className="absolute inset-y-0 left-0 w-[72px] sm:w-[84px] select-none"
                    >
                      {/* 时间轴节点圆点 */}
                      <div className="absolute left-[32px] sm:left-[38px] top-[130px] z-10 size-2 rounded-full border-2 border-surface bg-primary shadow-xs ring-4 ring-primary/15" />

                      {/* 粘性吸顶日期头 */}
                      <div className="sticky top-2 z-10 flex w-full flex-col items-center py-1 text-center">
                        <span className="font-serif text-3xl sm:text-4xl font-bold tracking-tight text-on-surface leading-none">
                          {dateParts.dayNumber}
                        </span>
                        <span className="mt-1 text-[10px] font-bold tracking-widest text-primary uppercase">
                          {dateParts.month}
                        </span>
                        <div className="mt-2 flex flex-col items-center gap-0.5 text-[11px] leading-tight text-on-surface-variant font-medium">
                          <span className="font-semibold text-on-surface">
                            {dateParts.relative}
                          </span>
                          <span className="text-[10px] text-outline">
                            {dateParts.weekday}
                          </span>
                        </div>
                        <span className="mt-1.5 font-mono text-[9px] text-outline/85 tracking-tight">
                          {todayStr}
                        </span>
                      </div>
                    </div>

                    {/* 右侧主体内容 */}
                    <div className="mb-4 h-[1px] bg-gradient-to-r from-theme-control-border/80 via-theme-control-border/30 to-transparent" />

                    <Panel
                      className="flex flex-col items-center justify-center gap-3 p-8 text-center rounded-2xl border border-theme-control-border/70 bg-surface/85 backdrop-blur-md shadow-xs"
                      variant="default"
                    >
                      <div className="grid size-12 place-items-center rounded-2xl border border-primary/20 bg-primary/10 text-primary shadow-xs">
                        <Clock3 size={22} />
                      </div>
                      <div className="flex flex-col gap-1 max-w-[420px]">
                        <h3 className="text-body-md font-bold text-on-surface">
                          {t("memory.recent.emptyTitle")}
                        </h3>
                        <p className="text-body-sm text-on-surface-variant leading-relaxed">
                          {t("memory.recent.emptyDescription")}
                        </p>
                      </div>
                      {stateView?.latestAttemptError && (
                        <div className="mt-2 inline-flex items-center gap-2 rounded-xl border border-status-issue/30 bg-status-issue/10 px-3 py-1.5 text-[12px] text-status-issue">
                          <AlertCircle size={14} />
                          <span>{stateView.latestAttemptError.message}</span>
                        </div>
                      )}
                    </Panel>
                  </section>
                );
              })()}
            </div>
          ) : (
            <Panel
              className="flex flex-col items-center justify-center gap-3 p-8 text-center rounded-2xl border border-theme-control-border/70 bg-surface/85 backdrop-blur-md shadow-xs"
              variant="default"
            >
              <div className="grid size-12 place-items-center rounded-2xl border border-primary/20 bg-primary/10 text-primary shadow-xs">
                <FolderOpen size={22} />
              </div>
              <div className="flex flex-col gap-1 max-w-[420px]">
                <h3 className="text-body-md font-bold text-on-surface">
                  {t("memory.recent.emptyTitle")}
                </h3>
                <p className="text-body-sm text-on-surface-variant leading-relaxed">
                  {t("memory.recent.emptyDescription")}
                </p>
              </div>
            </Panel>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-hidden">
      {/* 顶部控制栏 (M35-UI-05: 仅投影切换与状态展示，DOM 绝对无刷新/生成/窗口输入) */}
      <div className="flex shrink-0 items-center justify-between gap-3 px-1">
        <div className="flex items-center gap-3">
          <PillTabs<"time" | "project">
            activeId={view}
            items={pillItems}
            onSelect={(id) => setView(id)}
            size="sm"
          />
          <span className="text-caption font-medium text-on-surface-variant">
            {windowInfo}
          </span>
        </div>

        <div>{renderStatusBadge(stateView, t)}</div>
      </div>

      {/* 视图内容区 */}
      <div className="min-h-0 flex-1 overflow-auto pr-2 pb-8">
        {view === "time" ? (
          <div className="flex flex-col gap-8">
            {timeViewData.map(({ date, projectGroups }, dateIdx) => {
              const isLastDate = dateIdx === timeViewData.length - 1;
              const dateParts = parseDateRailParts(date, snapshot.targetWatermark, t);

              return (
                <section
                  className="relative min-w-0 pl-[84px] sm:pl-[98px]"
                  key={date}
                >
                  {/* 左侧垂直时间轴导轨 Date Rail */}
                  <div
                    aria-label={`${date}, ${dateParts.relative}`}
                    className="absolute inset-y-0 left-0 w-[72px] sm:w-[84px] select-none"
                  >
                    {/* 纵向时间连接线 */}
                    <div
                      className={clsx(
                        "absolute left-[35px] sm:left-[41px] top-[140px] w-[2px] bg-gradient-to-b from-primary/40 via-theme-control-border/60 to-theme-control-border/15",
                        isLastDate ? "bottom-4 mask-gradient" : "-bottom-8",
                      )}
                    />

                    {/* 时间轴节点圆点 */}
                    <div className="absolute left-[32px] sm:left-[38px] top-[130px] z-10 size-2 rounded-full border-2 border-surface bg-primary shadow-xs ring-4 ring-primary/15" />

                    {/* 粘性吸顶日期头 */}
                    <div className="sticky top-2 z-10 flex w-full flex-col items-center py-1 text-center">
                      {/* 大号衬线日期数字 */}
                      <span className="font-serif text-3xl sm:text-4xl font-bold tracking-tight text-on-surface leading-none">
                        {dateParts.dayNumber}
                      </span>

                      {/* 大写月份缩写 */}
                      <span className="mt-1 text-[10px] font-bold tracking-widest text-primary uppercase">
                        {dateParts.month}
                      </span>

                      {/* 相对时间与星期 */}
                      <div className="mt-2 flex flex-col items-center gap-0.5 text-[11px] leading-tight text-on-surface-variant font-medium">
                        <span className="font-semibold text-on-surface">
                          {dateParts.relative}
                        </span>
                        <span className="text-[10px] text-outline">
                          {dateParts.weekday}
                        </span>
                      </div>

                      {/* 完整日期标识 (满足机器测试与精确审计) */}
                      <span className="mt-1.5 font-mono text-[9px] text-outline/85 tracking-tight">
                        {date}
                      </span>

                      {/* 水位更新标记 */}
                      {dateParts.latestWatermark && (
                        <div className="mt-2 flex flex-col items-center border-t border-theme-control-border/60 pt-1.5 text-[10px] text-on-surface-variant">
                          <span className="font-mono font-semibold text-on-surface">
                            {dateParts.latestWatermark}
                          </span>
                          <span className="text-[9px] text-outline">
                            {t("memory.recent.latestUpdate") || "最近更新"}
                          </span>
                        </div>
                      )}
                    </div>
                  </div>

                  {/* 右侧主体内容 */}
                  {/* 顶部微细的水平连接线 */}
                  <div className="mb-4 h-[1px] bg-gradient-to-r from-theme-control-border/80 via-theme-control-border/30 to-transparent" />

                  {/* 该日期下的各个项目卡片 */}
                  <div className="flex flex-col gap-4">
                    {projectGroups.map(
                      ({ project, showWindowSummaryAndSuggestions, items }) => (
                        <Panel
                          className="flex flex-col gap-3.5 p-4 rounded-2xl border border-theme-control-border/70 bg-surface/85 backdrop-blur-md shadow-xs"
                          key={project.projectKey}
                          variant="default"
                        >
                          {/* 项目标题栏 */}
                          <div className="flex items-center justify-between gap-3 border-b border-theme-control-border/60 pb-3">
                            <div className="flex items-center gap-2.5 min-w-0">
                              <span className="grid size-7 shrink-0 place-items-center rounded-xl border border-primary/20 bg-primary/10 text-primary shadow-xs">
                                <FolderOpen size={14} />
                              </span>
                              <span className="truncate text-body-md font-bold text-on-surface">
                                {project.projectTitle}
                              </span>
                              {project.projectPath && (
                                <span className="hidden sm:inline-block max-w-[240px] truncate rounded-md border border-theme-control-border/60 bg-theme-control/30 px-1.5 py-0.5 font-mono text-[11px] text-on-surface-variant">
                                  {project.projectPath}
                                </span>
                              )}
                            </div>
                            <span className="shrink-0 text-caption font-medium text-on-surface-variant">
                              {items.length} {t("memory.recent.items")}
                            </span>
                          </div>

                          {/* 工作摘要与下一步 (M35-PROJ-02: 仅在 latestActivityAt 当日渲染一次) */}
                          {showWindowSummaryAndSuggestions && (
                            <>
                              {project.summary && (
                                <div className="flex flex-col gap-1.5 rounded-xl border border-theme-control-border/50 bg-theme-control/15 p-3">
                                  <span className="text-[11px] font-bold uppercase tracking-wider text-on-surface-variant">
                                    {t("memory.recent.whatChanged")}
                                  </span>
                                  <div className="text-body-sm text-on-surface leading-relaxed">
                                    <MarkdownContent value={project.summary} />
                                  </div>
                                </div>
                              )}

                              <SuggestedNextSteps project={project} t={t} />
                            </>
                          )}

                          {/* 当日条目列表 */}
                          {items.length > 0 && (
                            <div className="flex flex-col gap-2.5">
                              {items.map((item) => (
                                <RecentItemCard
                                  isExpanded={expandedItemIds.has(item.itemId)}
                                  item={item}
                                  key={item.itemId}
                                  onNavigateSession={onNavigateSession}
                                  onToggle={() => toggleItem(item.itemId)}
                                  t={t}
                                  targetWatermark={snapshot.targetWatermark}
                                />
                              ))}
                            </div>
                          )}
                        </Panel>
                      ),
                    )}
                  </div>
                </section>
              );
            })}
          </div>
        ) : (
          <div className="flex flex-col gap-5">
            {projectViewData.map(({ project, items }) => (
              <Panel
                className="flex flex-col gap-3.5 p-4.5 rounded-2xl border border-theme-control-border/70 bg-surface/85 backdrop-blur-md shadow-xs"
                key={project.projectKey}
                variant="default"
              >
                {/* 项目标题与元数据 */}
                <div className="flex items-center justify-between gap-3 border-b border-theme-control-border/60 pb-3">
                  <div className="flex items-center gap-2.5 min-w-0">
                    <span className="grid size-7 shrink-0 place-items-center rounded-xl border border-primary/20 bg-primary/10 text-primary shadow-xs">
                      <FolderOpen size={14} />
                    </span>
                    <span className="truncate text-body-md font-bold text-on-surface">
                      {project.projectTitle}
                    </span>
                    {project.projectPath && (
                      <span className="hidden sm:inline-block max-w-[260px] truncate rounded-md border border-theme-control-border/60 bg-theme-control/30 px-1.5 py-0.5 font-mono text-[11px] text-on-surface-variant">
                        {project.projectPath}
                      </span>
                    )}
                  </div>
                  <div className="flex shrink-0 items-center gap-2 text-caption text-on-surface-variant">
                    <span>{formatTime(project.latestActivityAt)}</span>
                    <span>·</span>
                    <span>
                      {items.length} {t("memory.recent.items")}
                    </span>
                  </div>
                </div>

                {project.summary && (
                  <div className="flex flex-col gap-1.5 rounded-xl border border-theme-control-border/50 bg-theme-control/15 p-3">
                    <span className="text-[11px] font-bold uppercase tracking-wider text-on-surface-variant">
                      {t("memory.recent.whatChanged")}
                    </span>
                    <div className="text-body-sm text-on-surface leading-relaxed">
                      <MarkdownContent value={project.summary} />
                    </div>
                  </div>
                )}

                <SuggestedNextSteps project={project} t={t} />

                <div className="flex flex-col gap-2.5">
                  {items.map((item) => (
                    <RecentItemCard
                      isExpanded={expandedItemIds.has(item.itemId)}
                      item={item}
                      key={item.itemId}
                      onNavigateSession={onNavigateSession}
                      onToggle={() => toggleItem(item.itemId)}
                      t={t}
                      targetWatermark={snapshot.targetWatermark}
                    />
                  ))}
                </div>
              </Panel>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function SuggestedNextSteps({
  project,
  t,
}: {
  project: RecentProjectView;
  t: Translator;
}) {
  const suggestions = useMemo(() => {
    return project.items
      .filter((item) => typeof item.recommendationRank === "number")
      .sort(
        (a, b) =>
          (a.recommendationRank ?? 99) - (b.recommendationRank ?? 99),
      )
      .slice(0, 3);
  }, [project.items]);

  return (
    <div className="flex flex-col gap-2 rounded-xl border border-primary/20 bg-primary/5 p-3">
      <div className="flex items-center gap-1.5 text-caption font-semibold text-on-surface">
        <Sparkles size={13} className="text-primary" />
        <span>{t("memory.recent.suggestedNext")}</span>
      </div>
      {suggestions.length === 0 ? (
        <p className="text-body-sm text-on-surface-variant/80 italic">
          {t("memory.recent.noSuggestions")}
        </p>
      ) : (
        <ol className="flex flex-col gap-2 text-body-sm text-on-surface">
          {suggestions.map((item, idx) => (
            <li className="flex items-start gap-2.5" key={item.itemId}>
              <span className="grid size-5 shrink-0 place-items-center rounded-full bg-primary/20 text-[11px] font-bold text-primary shadow-xs">
                {idx + 1}
              </span>
              <div className="flex flex-col">
                <span className="font-semibold text-on-surface">
                  {item.title}
                </span>
                {item.summary && (
                  <span className="text-caption text-on-surface-variant mt-0.5">
                    {item.summary}
                  </span>
                )}
              </div>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}

function RecentItemCard({
  item,
  isExpanded,
  onToggle,
  onNavigateSession,
  targetWatermark,
  t,
}: {
  item: RecentMemoryItemView;
  isExpanded: boolean;
  onToggle: () => void;
  onNavigateSession?: (target: MemoryNavigationTarget) => void;
  targetWatermark: string;
  t: Translator;
}) {
  const categoryBadgeClass = matchCategory(item.category);
  const dotColorClass = matchCategoryDot(item.category);

  return (
    <div className="flex flex-col rounded-xl border border-theme-control-border/60 bg-theme-control/20 transition-all duration-150 hover:border-theme-control-border/90 hover:bg-theme-control/30 shadow-xs">
      {/* 头部摘要栏 (可点击展开/收起) */}
      <button
        aria-expanded={isExpanded}
        aria-label={
          isExpanded
            ? `${t("memory.recent.collapseItem")}: ${item.title}`
            : `${t("memory.recent.expandItem")}: ${item.title}`
        }
        className="flex w-full items-start justify-between gap-3 p-3.5 text-left focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 cursor-pointer rounded-xl"
        onClick={onToggle}
        type="button"
      >
        <div className="flex min-w-0 flex-1 items-start gap-3">
          {/* 类别圆点指示 */}
          <span
            className={clsx(
              "mt-1.5 size-2 shrink-0 rounded-full ring-4 shadow-xs",
              dotColorClass,
            )}
          />

          <div className="flex min-w-0 flex-1 flex-col gap-1.5">
            <div className="flex flex-wrap items-center gap-2">
              <span
                className={clsx(
                  "rounded-full border px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider",
                  categoryBadgeClass,
                )}
              >
                {item.category}
              </span>
              <span className="rounded-full border border-theme-control-border/70 bg-theme-control/40 px-2 py-0.5 text-caption font-medium text-on-surface-variant">
                {item.status}
              </span>
              <span className="text-caption text-on-surface-variant">
                {formatTime(item.occurredAt)}
              </span>
              <AvailabilityBadge availability={item.sourceAvailability} t={t} />
            </div>

            <h4 className="text-body-md font-semibold text-on-surface leading-snug">
              {item.title}
            </h4>

            {item.summary && (
              <div className="text-body-sm text-on-surface-variant leading-relaxed">
                <MarkdownContent value={item.summary} />
              </div>
            )}
          </div>
        </div>

        <span className="mt-1 grid size-7 shrink-0 place-items-center rounded-full border border-theme-control-border/70 bg-theme-control/50 text-on-surface-variant transition-colors hover:text-on-surface">
          {isExpanded ? <ChevronUp size={15} /> : <ChevronDown size={15} />}
        </span>
      </button>

      {/* 展开详细信息 (M35-UI-03: rationale, watermark, sessions) */}
      {isExpanded && (
        <div className="flex flex-col gap-3.5 border-t border-theme-control-border/50 bg-theme-control/10 p-4">
          {item.rationale && (
            <div className="flex flex-col gap-1.5">
              <span className="text-[11px] font-bold uppercase tracking-wider text-on-surface-variant">
                {t("memory.recent.why")}
              </span>
              <div className="rounded-xl border border-theme-control-border/60 bg-surface-elevated/70 p-3 text-body-sm text-on-surface leading-relaxed shadow-xs">
                {item.rationale}
              </div>
            </div>
          )}

          <div className="flex items-center gap-2 text-caption text-on-surface-variant">
            <span className="font-semibold text-on-surface">{t("memory.recent.watermark")}:</span>
            <span className="font-mono">{formatTime(targetWatermark)}</span>
          </div>

          {item.sessionReferences.length > 0 && (
            <div className="flex flex-col gap-2">
              <span className="text-[11px] font-bold uppercase tracking-wider text-on-surface-variant">
                {t("memory.recent.sessions")} ({item.sessionReferences.length})
              </span>
              <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
                {item.sessionReferences.map((ref) => (
                  <SessionReferenceCard
                    key={`${ref.sourceId}:${ref.sessionId}`}
                    onNavigateSession={onNavigateSession}
                    reference={ref}
                    t={t}
                  />
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function SessionReferenceCard({
  reference,
  onNavigateSession,
  t,
}: {
  reference: RecentSessionReferenceView;
  onNavigateSession?: (target: MemoryNavigationTarget) => void;
  t: Translator;
}) {
  const isAvailable = reference.available;

  const handleClick = () => {
    if (!isAvailable) return;
    onNavigateSession?.({
      record_kind: "session",
      source_id: reference.sourceId,
      session_id: reference.sessionId,
      question_id: null,
      turn_id: null,
      part_id: null,
      block_id: null,
    });
  };

  return (
    <div
      aria-disabled={!isAvailable}
      className={clsx(
        "flex flex-col gap-1.5 rounded-xl border p-3 transition-all text-left",
        isAvailable
          ? "cursor-pointer border-theme-control-border/70 bg-surface-elevated/60 hover:border-primary/50 hover:bg-surface-elevated/90 shadow-xs"
          : "cursor-not-allowed border-theme-control-border/40 bg-theme-control/15 opacity-65",
      )}
      onClick={handleClick}
      onKeyDown={(e) => {
        if (isAvailable && (e.key === "Enter" || e.key === " ")) {
          e.preventDefault();
          handleClick();
        }
      }}
      role={isAvailable ? "button" : undefined}
      tabIndex={isAvailable ? 0 : undefined}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="rounded-md border border-primary/25 bg-primary/10 px-1.5 py-0.5 font-mono text-[10px] font-bold text-primary">
          {reference.sourceAgent}
        </span>
        <div className="flex items-center gap-1">
          {isAvailable ? (
            <span className="inline-flex items-center gap-1 text-[11px] font-medium text-status-add">
              <span className="size-1.5 rounded-full bg-status-add" />
              <span>{t("memory.recent.sourceAvailable")}</span>
              <ExternalLink size={11} />
            </span>
          ) : (
            <span className="inline-flex items-center gap-1 text-[11px] text-on-surface-variant">
              <span className="size-1.5 rounded-full bg-outline" />
              <span>
                {reference.unavailableReason ||
                  t("memory.recent.sourceUnavailable")}
              </span>
            </span>
          )}
        </div>
      </div>

      <div className="line-clamp-2 text-body-sm font-semibold text-on-surface">
        {reference.sessionTitle}
      </div>

      <div className="text-[11px] text-on-surface-variant">
        {formatTime(reference.lastActivityAt)}
      </div>
    </div>
  );
}

function AvailabilityBadge({
  availability,
  t,
}: {
  availability: string;
  t: Translator;
}) {
  if (availability === "available") {
    return null;
  }
  if (availability === "partially_unavailable") {
    return (
      <span className="rounded-full border border-status-issue/40 bg-status-issue/10 px-2 py-0.5 text-[10px] font-medium text-status-issue">
        {t("memory.recent.partiallyUnavailable")}
      </span>
    );
  }
  return (
    <span className="rounded-full border border-theme-control-border/60 bg-theme-control/40 px-2 py-0.5 text-[10px] font-medium text-on-surface-variant">
      {t("memory.recent.sourceUnavailable")}
    </span>
  );
}

function renderStatusBadge(
  stateView: RecentMemoryStateView | null,
  t: Translator,
) {
  if (!stateView) return null;
  const { status, snapshot, latestAttemptError } = stateView;

  if (status === "update_failed" || (latestAttemptError && snapshot)) {
    return (
      <div className="inline-flex items-center gap-1.5 rounded-full border border-status-issue/40 bg-status-issue/10 px-2.5 py-0.5 text-caption font-medium text-status-issue">
        <AlertCircle size={12} />
        <span>{t("memory.recent.updateIncomplete")}</span>
      </div>
    );
  }

  if (snapshot?.publicationKind === "reused") {
    return (
      <div className="inline-flex items-center gap-1.5 rounded-full border border-theme-control-border/80 bg-theme-control/50 px-2.5 py-0.5 text-caption font-medium text-on-surface-variant shadow-xs">
        <Copy className="text-primary" size={12} />
        <span>{t("memory.recent.reused")}</span>
      </div>
    );
  }

  if (snapshot?.publicationKind === "generated" || status === "ready") {
    return (
      <div className="inline-flex items-center gap-1.5 rounded-full border border-status-add/40 bg-status-add/10 px-2.5 py-0.5 text-caption font-medium text-status-add">
        <CheckCircle2 size={12} />
        <span>{t("memory.recent.updated")}</span>
      </div>
    );
  }

  return null;
}

function matchCategory(cat: string): string {
  switch (cat.toLowerCase()) {
    case "decision":
      return "border-primary/40 bg-primary/10 text-primary";
    case "research":
      return "border-theme-control-border bg-theme-control/50 text-theme-nav-active-fg";
    case "verification":
      return "border-status-add/40 bg-status-add/10 text-status-add";
    case "blocker":
      return "border-status-remove/40 bg-status-remove/10 text-status-remove";
    case "follow_up":
      return "border-theme-control-border bg-theme-control/40 text-on-surface";
    default:
      return "border-theme-control-border bg-theme-control/30 text-on-surface-variant";
  }
}

function matchCategoryDot(cat: string): string {
  switch (cat.toLowerCase()) {
    case "decision":
      return "bg-primary ring-primary/20";
    case "research":
      return "bg-status-update ring-status-update/20";
    case "verification":
      return "bg-status-add ring-status-add/20";
    case "blocker":
      return "bg-status-remove ring-status-remove/20";
    default:
      return "bg-on-surface-variant ring-on-surface-variant/20";
  }
}

interface DateRailParts {
  dayNumber: string;
  month: string;
  weekday: string;
  relative: string;
  latestWatermark?: string;
}

function parseDateRailParts(
  dateStr: string,
  targetWatermark: string | undefined,
  t: Translator,
): DateRailParts {
  const parts = dateStr.split("-");
  const year = parseInt(parts[0], 10);
  const monthIdx = parseInt(parts[1], 10) - 1;
  const day = parseInt(parts[2], 10);

  const dateObj = new Date(year, monthIdx, day);
  const dayNumber = isNaN(day) ? dateStr : String(day);

  const monthNames = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN",
    "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
  ];
  const month = monthNames[monthIdx] ?? (monthIdx + 1) + "月";

  const weekdaysZh = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];
  const weekdaysEn = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
  const isZh = t("memory.recent.today") === "今天";
  const weekday = isZh
    ? weekdaysZh[dateObj.getDay()] ?? ""
    : weekdaysEn[dateObj.getDay()] ?? "";

  const today = new Date();
  const todayZero = new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime();
  const targetZero = new Date(year, monthIdx, day).getTime();
  const diffDays = Math.round((todayZero - targetZero) / (1000 * 60 * 60 * 24));

  let relative = "";
  if (diffDays === 0) {
    relative = t("memory.recent.today");
  } else if (diffDays === 1) {
    relative = t("memory.recent.yesterday");
  } else if (diffDays === 2) {
    relative = t("memory.recent.beforeYesterday");
  } else if (diffDays > 2 && diffDays < 30) {
    relative = t("memory.recent.daysAgo", { days: diffDays });
  } else {
    relative = `${monthIdx + 1}/${day}`;
  }

  let latestWatermark: string | undefined;
  if (targetWatermark) {
    const wmDate = targetWatermark.split("T")[0];
    if (wmDate === dateStr) {
      const wDateObj = new Date(targetWatermark);
      if (!isNaN(wDateObj.valueOf())) {
        const hh = String(wDateObj.getHours()).padStart(2, "0");
        const mm = String(wDateObj.getMinutes()).padStart(2, "0");
        latestWatermark = `${hh}:${mm}`;
      }
    }
  }

  return {
    dayNumber,
    month,
    weekday,
    relative,
    latestWatermark,
  };
}

function formatWatermark(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.valueOf())
    ? value
    : `${date.getMonth() + 1}月${date.getDate()}日 ${date.getHours().toString().padStart(2, "0")}:${date.getMinutes().toString().padStart(2, "0")}`;
}

function formatTime(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? value : date.toLocaleString();
}
