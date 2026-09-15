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

  if (loading && !snapshot) {
    return <AppSkeleton label={t("common.loading")} layout="list" />;
  }

  if (!snapshot && stateView?.status === "generating") {
    return <AppSkeleton label={t("memory.recent.generating")} layout="list" />;
  }

  if (!snapshot || snapshot.projects.length === 0) {
    return (
      <EmptyState
        className="min-h-0 flex-1"
        description={t("memory.recent.emptyDescription")}
        icon={<Clock3 size={20} />}
        title={t("memory.recent.emptyTitle")}
      />
    );
  }

  const windowInfo = `${snapshot.windowHours}h · ${formatWatermark(snapshot.targetWatermark)}`;

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
      <div className="min-h-0 flex-1 overflow-auto pr-1">
        {view === "time" ? (
          <div className="flex flex-col gap-6">
            {timeViewData.map(({ date, projectGroups }) => (
              <section className="flex flex-col gap-3" key={date}>
                {/* 日期轨道锚点 */}
                <div className="sticky top-0 z-10 flex items-center gap-2 rounded-xl border border-theme-control-border/70 bg-surface/90 px-3 py-1.5 backdrop-blur-md shadow-xs">
                  <Calendar size={14} className="text-primary" />
                  <span className="text-body-sm font-semibold text-on-surface">
                    {date}
                  </span>
                </div>

                <div className="flex flex-col gap-4 pl-1">
                  {projectGroups.map(
                    ({ project, showWindowSummaryAndSuggestions, items }) => (
                      <Panel
                        className="flex flex-col gap-3 p-4"
                        key={project.projectKey}
                        variant="default"
                      >
                        {/* 项目标题栏 */}
                        <div className="flex items-center justify-between gap-3 border-b border-theme-control-border/60 pb-3">
                          <div className="flex items-center gap-2">
                            <span className="grid size-6 place-items-center rounded-lg border border-theme-control-border bg-theme-control/60 text-primary">
                              <FolderOpen size={13} />
                            </span>
                            <span className="text-body-md font-semibold text-on-surface">
                              {project.projectTitle}
                            </span>
                          </div>
                          <span className="text-caption text-on-surface-variant">
                            {items.length} {t("memory.recent.items")}
                          </span>
                        </div>

                        {/* 工作摘要与下一步 (M35-PROJ-02: 仅在 latestActivityAt 当日渲染一次) */}
                        {showWindowSummaryAndSuggestions && (
                          <>
                            {project.summary && (
                              <div className="flex flex-col gap-1.5">
                                <span className="text-caption font-semibold uppercase tracking-wide text-on-surface-variant">
                                  {t("memory.recent.whatChanged")}
                                </span>
                                <div className="text-body-sm text-on-surface-variant">
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
            ))}
          </div>
        ) : (
          <div className="flex flex-col gap-4">
            {projectViewData.map(({ project, items }) => (
              <Panel
                className="flex flex-col gap-3 p-4"
                key={project.projectKey}
                variant="default"
              >
                {/* 项目标题与元数据 */}
                <div className="flex items-center justify-between gap-3 border-b border-theme-control-border/60 pb-3">
                  <div className="flex items-center gap-2">
                    <span className="grid size-6 place-items-center rounded-lg border border-theme-control-border bg-theme-control/60 text-primary">
                      <FolderOpen size={13} />
                    </span>
                    <span className="text-body-md font-semibold text-on-surface">
                      {project.projectTitle}
                    </span>
                    {project.projectPath && (
                      <span className="rounded-md border border-theme-control-border/60 bg-theme-control/30 px-1.5 py-0.5 font-mono text-[11px] text-on-surface-variant">
                        {project.projectPath}
                      </span>
                    )}
                  </div>
                  <div className="flex items-center gap-2 text-caption text-on-surface-variant">
                    <span>{formatTime(project.latestActivityAt)}</span>
                    <span>·</span>
                    <span>
                      {items.length} {t("memory.recent.items")}
                    </span>
                  </div>
                </div>

                {project.summary && (
                  <div className="flex flex-col gap-1.5">
                    <span className="text-caption font-semibold uppercase tracking-wide text-on-surface-variant">
                      {t("memory.recent.whatChanged")}
                    </span>
                    <div className="text-body-sm text-on-surface-variant">
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
    <div className="flex flex-col gap-2 rounded-xl border border-theme-control-border/60 bg-theme-control/25 p-3">
      <div className="flex items-center gap-1.5 text-caption font-semibold text-on-surface">
        <Sparkles size={13} className="text-primary" />
        <span>{t("memory.recent.suggestedNext")}</span>
      </div>
      {suggestions.length === 0 ? (
        <p className="text-body-sm text-on-surface-variant/80 italic">
          {t("memory.recent.noSuggestions")}
        </p>
      ) : (
        <ol className="flex flex-col gap-1.5 text-body-sm text-on-surface">
          {suggestions.map((item, idx) => (
            <li className="flex items-start gap-2" key={item.itemId}>
              <span className="grid size-5 shrink-0 place-items-center rounded-full bg-theme-control/80 text-[11px] font-bold text-primary">
                {idx + 1}
              </span>
              <div className="flex flex-col">
                <span className="font-medium text-on-surface">
                  {item.title}
                </span>
                {item.summary && (
                  <span className="text-caption text-on-surface-variant">
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

  return (
    <div className="flex flex-col rounded-xl border border-theme-control-border/60 bg-theme-control/20 transition-all hover:border-theme-control-border/90 hover:bg-theme-control/30">
      {/* 头部摘要栏 (可点击展开/收起) */}
      <button
        aria-expanded={isExpanded}
        aria-label={
          isExpanded
            ? `${t("memory.recent.collapseItem")}: ${item.title}`
            : `${t("memory.recent.expandItem")}: ${item.title}`
        }
        className="flex w-full items-start justify-between gap-3 p-3 text-left focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50 cursor-pointer rounded-xl"
        onClick={onToggle}
        type="button"
      >
        <div className="flex min-w-0 flex-1 flex-col gap-1.5">
          <div className="flex flex-wrap items-center gap-2">
            <span
              className={clsx(
                "rounded-full border px-2 py-0.5 text-[11px] font-semibold uppercase tracking-wider",
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

          <h4 className="text-body-md font-semibold text-on-surface">
            {item.title}
          </h4>

          {item.summary && (
            <div className="text-body-sm text-on-surface-variant">
              <MarkdownContent value={item.summary} />
            </div>
          )}
        </div>

        <span className="mt-1 grid size-7 shrink-0 place-items-center rounded-full border border-theme-control-border/70 bg-theme-control/50 text-on-surface-variant transition-colors hover:text-on-surface">
          {isExpanded ? <ChevronUp size={15} /> : <ChevronDown size={15} />}
        </span>
      </button>

      {/* 展开详细信息 (M35-UI-03: rationale, watermark, sessions) */}
      {isExpanded && (
        <div className="flex flex-col gap-3 border-t border-theme-control-border/50 bg-theme-control/10 p-3.5">
          {item.rationale && (
            <div className="flex flex-col gap-1">
              <span className="text-caption font-semibold uppercase tracking-wide text-on-surface-variant">
                {t("memory.recent.why")}
              </span>
              <div className="rounded-lg border border-theme-control-border/50 bg-surface-elevated/40 p-2.5 text-body-sm text-on-surface">
                {item.rationale}
              </div>
            </div>
          )}

          <div className="flex items-center gap-2 text-caption text-on-surface-variant">
            <span className="font-medium">{t("memory.recent.watermark")}:</span>
            <span className="font-mono">{formatTime(targetWatermark)}</span>
          </div>

          {item.sessionReferences.length > 0 && (
            <div className="flex flex-col gap-2">
              <span className="text-caption font-semibold uppercase tracking-wide text-on-surface-variant">
                {t("memory.recent.sessions")} ({item.sessionReferences.length})
              </span>
              <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
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
        "flex flex-col gap-1.5 rounded-xl border p-2.5 transition-all text-left",
        isAvailable
          ? "cursor-pointer border-theme-control-border/70 bg-surface-elevated/50 hover:border-primary/50 hover:bg-surface-elevated/80 shadow-xs"
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
        <span className="rounded-md border border-theme-control-border/80 bg-theme-control/50 px-1.5 py-0.5 font-mono text-[10px] font-medium text-primary">
          {reference.sourceAgent}
        </span>
        <div className="flex items-center gap-1">
          {isAvailable ? (
            <span className="inline-flex items-center gap-1 text-[11px] text-status-add">
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

      <div className="line-clamp-2 text-body-sm font-medium text-on-surface">
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
      return "border-theme-tag-border bg-primary/10 text-primary";
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
