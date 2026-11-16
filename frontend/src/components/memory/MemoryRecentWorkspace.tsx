import {
  AlertCircle,
  ChevronDown,
  ChevronUp,
  Clock3,
  Filter,
  FolderOpen,
  GitBranch,
  Layers,
  RefreshCw,
  RotateCcw,
  Sparkles,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useMemoryTasks } from "../../app/backgroundTasks/MemoryTaskProvider";
import type { Translator } from "../../i18n/I18nProvider";
import {
  emptyMemoryScope,
  getMemoryProjectState,
  getRecentMemorySnapshot,
  rebuildMemoryScope,
  refreshMemoryRecentState,
  subscribeMemoryTasks,
  updateMemoryNodeStatus,
} from "../../services/memory";
import type {
  MemoryNavigationTarget,
  MemoryNodeStatus,
  MemoryProjectStateView,
  RecentMemoryEvent,
  RecentMemoryStateView,
  RecentTreeNodeView,
} from "../../types/memory";
import {
  DataToolbar,
  DebouncedToolbarSearch,
  ToolbarActionButton,
  ToolbarCluster,
  ToolbarSingleSelectDropdown,
} from "../common/DataToolbar";
import { PillTabs, type PillTabItem } from "../common/PillTabs";
import { Badge } from "../foundation/Badge";
import { EmptyState } from "../foundation/EmptyState";
import { Panel } from "../foundation/Panel";
import { AppSkeleton } from "../foundation/skeleton";
import { Button } from "../ui/button";
import { RecentTopicTree, parseNodeAnchor } from "./RecentTopicTree";
import { MemoryDateRailView } from "./recent/MemoryDateRailView";
import { MemoryNodeCard } from "./recent/MemoryNodeCard";
import {
  computeRecentMetrics,
  extractUnexploredEntries,
  filterRecentProjects,
  type RecentMetricsSummary,
  type RecentStatusFilter,
  type RecentViewMode,
} from "./recent/recentModel";

export { parseNodeAnchor };

export interface MemoryRecentWorkspaceProps {
  onEventOpen?: (event: RecentMemoryEvent) => void;
  onMetricsChange?: (metrics: RecentMetricsSummary) => void;
  onNavigateSession?: (target: MemoryNavigationTarget) => void;
  t: Translator;
}

type WindowChoice = "24h" | "48h" | "72h";

interface DismissedUndoRecord {
  nodeId: string;
  previousStatus: MemoryNodeStatus;
  projectKey: string;
  summary: string;
}

export function MemoryRecentWorkspace({
  onEventOpen: _legacyOnEventOpen,
  onMetricsChange,
  onNavigateSession,
  t,
}: MemoryRecentWorkspaceProps) {
  const [view, setView] = useState<RecentViewMode>("project");
  const [stateView, setStateView] = useState<RecentMemoryStateView | null>(
    null,
  );
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [reslicing, setReslicing] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [statusFilter, setStatusFilter] = useState<RecentStatusFilter>("all");
  const [selectedProjectKey, setSelectedProjectKey] = useState<string>("all");
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedWindow, setSelectedWindow] = useState<WindowChoice>("72h");
  const [focusExpanded, setFocusExpanded] = useState(true);
  const [updatingNodeId, setUpdatingNodeId] = useState<string | null>(null);
  const [dismissedUndo, setDismissedUndo] =
    useState<DismissedUndoRecord | null>(null);
  const [expandedHistorySlugs, setExpandedHistorySlugs] = useState<Set<string>>(
    () => new Set(),
  );
  const [fullProjectStates, setFullProjectStates] = useState<
    Record<string, MemoryProjectStateView | null>
  >({});
  const [loadingProjectSlugs, setLoadingProjectSlugs] = useState<Set<string>>(
    () => new Set(),
  );

  const { task } = useMemoryTasks();
  const isMemoryTaskRunning =
    generating || task?.status === "pending" || task?.status === "running";

  const applySnapshot = useCallback(
    (nextState: RecentMemoryStateView) => {
      setStateView(nextState);
      const windowHours = nextState.snapshot?.windowHours ?? 72;
      const normalizedHours: WindowChoice =
        windowHours <= 24 ? "24h" : windowHours <= 48 ? "48h" : "72h";
      setSelectedWindow(normalizedHours);
      onMetricsChange?.(computeRecentMetrics(nextState));
    },
    [onMetricsChange],
  );

  const loadSnapshot = useCallback(
    async ({ silent = false }: { silent?: boolean } = {}) => {
      if (!silent) {
        setLoading(true);
      }
      setLoadError(null);
      try {
        const snapshot = await getRecentMemorySnapshot();
        applySnapshot(snapshot);
      } catch (error) {
        setLoadError(
          error instanceof Error ? error.message : String(error),
        );
      } finally {
        if (!silent) {
          setLoading(false);
        }
      }
    },
    [applySnapshot],
  );

  useEffect(() => {
    void loadSnapshot();
  }, [loadSnapshot]);

  useEffect(() => {
    let active = true;
    let cleanup: (() => void) | undefined;

    Promise.resolve(
      subscribeMemoryTasks?.(() => {
        if (active) {
          void loadSnapshot({ silent: true });
        }
      }),
    )
      .then((unlisten) => {
        if (active && typeof unlisten === "function") {
          cleanup = unlisten;
        } else if (typeof unlisten === "function") {
          unlisten();
        }
      })
      .catch(() => {
        // Ignore task subscription setup errors in non-Tauri previews.
      });

    return () => {
      active = false;
      cleanup?.();
    };
  }, [loadSnapshot]);

  const handleReslice = useCallback(
    async (windowOverride?: WindowChoice) => {
      const targetWindow = windowOverride ?? selectedWindow;
      setReslicing(true);
      setActionError(null);
      try {
        const nextSnapshot = await refreshMemoryRecentState(targetWindow);
        applySnapshot(nextSnapshot);
      } catch (error) {
        setActionError(
          t("memory.recent.actionError", {
            message: error instanceof Error ? error.message : String(error),
          }),
        );
      } finally {
        setReslicing(false);
      }
    },
    [applySnapshot, selectedWindow, t],
  );

  const handleGenerateNow = useCallback(async () => {
    setGenerating(true);
    setActionError(null);
    try {
      await rebuildMemoryScope(emptyMemoryScope(), {
        target: "recent",
        reason: "manual",
      });
      const snapshot = await getRecentMemorySnapshot();
      applySnapshot(snapshot);
    } catch (error) {
      setActionError(
        t("memory.recent.actionError", {
          message: error instanceof Error ? error.message : String(error),
        }),
      );
    } finally {
      setGenerating(false);
    }
  }, [applySnapshot, t]);

  const handleToggleProjectHistory = useCallback(
    async (projectKey: string) => {
      const isCurrentlyExpanded = expandedHistorySlugs.has(projectKey);
      setExpandedHistorySlugs((prev) => {
        const next = new Set(prev);
        if (isCurrentlyExpanded) {
          next.delete(projectKey);
        } else {
          next.add(projectKey);
        }
        return next;
      });

      if (!isCurrentlyExpanded && fullProjectStates[projectKey] === undefined) {
        setLoadingProjectSlugs((prev) => {
          const next = new Set(prev);
          next.add(projectKey);
          return next;
        });
        try {
          const projectState = await getMemoryProjectState(projectKey);
          setFullProjectStates((prev) => ({
            ...prev,
            [projectKey]: projectState,
          }));
        } catch (error) {
          setActionError(
            t("memory.recent.actionError", {
              message: error instanceof Error ? error.message : String(error),
            }),
          );
        } finally {
          setLoadingProjectSlugs((prev) => {
            const next = new Set(prev);
            next.delete(projectKey);
            return next;
          });
        }
      }
    },
    [expandedHistorySlugs, fullProjectStates, t],
  );

  const handleStatusChange = useCallback(
    async (
      node: RecentTreeNodeView,
      nextStatus: MemoryNodeStatus,
      projectKey: string,
    ) => {
      setUpdatingNodeId(node.id);
      setActionError(null);
      try {
        await updateMemoryNodeStatus({
          nodeId: node.id,
          status: nextStatus,
          projectSlug: projectKey,
          window: selectedWindow,
        });

        if (nextStatus === "dismissed") {
          setDismissedUndo({
            nodeId: node.id,
            previousStatus: node.status,
            projectKey,
            summary: node.summary,
          });
        } else if (dismissedUndo?.nodeId === node.id) {
          setDismissedUndo(null);
        }

        const refreshed = await getRecentMemorySnapshot();
        applySnapshot(refreshed);

        if (expandedHistorySlugs.has(projectKey)) {
          const fullState = await getMemoryProjectState(projectKey);
          setFullProjectStates((prev) => ({
            ...prev,
            [projectKey]: fullState,
          }));
        }
      } catch (error) {
        setActionError(
          t("memory.recent.actionError", {
            message: error instanceof Error ? error.message : String(error),
          }),
        );
      } finally {
        setUpdatingNodeId(null);
      }
    },
    [
      applySnapshot,
      dismissedUndo?.nodeId,
      expandedHistorySlugs,
      selectedWindow,
      t,
    ],
  );

  const handleUndoDismiss = useCallback(async () => {
    if (!dismissedUndo) return;
    const target = dismissedUndo;
    setDismissedUndo(null);
    setUpdatingNodeId(target.nodeId);
    setActionError(null);
    try {
      await updateMemoryNodeStatus({
        nodeId: target.nodeId,
        status: target.previousStatus,
        projectSlug: target.projectKey,
        window: selectedWindow,
      });
      const refreshed = await getRecentMemorySnapshot();
      applySnapshot(refreshed);
      if (expandedHistorySlugs.has(target.projectKey)) {
        const fullState = await getMemoryProjectState(target.projectKey);
        setFullProjectStates((prev) => ({
          ...prev,
          [target.projectKey]: fullState,
        }));
      }
    } catch (error) {
      setActionError(
        t("memory.recent.actionError", {
          message: error instanceof Error ? error.message : String(error),
        }),
      );
    } finally {
      setUpdatingNodeId(null);
    }
  }, [applySnapshot, dismissedUndo, expandedHistorySlugs, selectedWindow, t]);

  const rawProjects = useMemo(
    () => stateView?.snapshot?.projects ?? [],
    [stateView],
  );

  const viewTabs: PillTabItem<RecentViewMode>[] = useMemo(
    () => [
      {
        id: "project",
        label: t("memory.recent.projectView"),
        icon: <Layers size={14} />,
      },
      {
        id: "time",
        label: t("memory.recent.timeView"),
        icon: <Clock3 size={14} />,
      },
    ],
    [t],
  );

  const statusOptions = useMemo(
    () => [
      { value: "all" as const, label: t("memory.recent.filterStatusAll") },
      {
        value: "unexplored" as const,
        label: t("memory.recent.filterStatusUnexplored"),
      },
      {
        value: "completed" as const,
        label: t("memory.recent.filterStatusCompleted"),
      },
    ],
    [t],
  );

  const projectOptions = useMemo(
    () => [
      { value: "all", label: t("memory.recent.filterProjectAll") },
      ...rawProjects.map((project) => ({
        value: project.projectKey,
        label: project.projectTitle,
      })),
    ],
    [rawProjects, t],
  );

  const windowOptions = useMemo(
    () => [
      { value: "24h" as const, label: "24h" },
      { value: "48h" as const, label: "48h" },
      { value: "72h" as const, label: "72h" },
    ],
    [],
  );

  const filteredProjects = useMemo(
    () =>
      filterRecentProjects(rawProjects, {
        projectKey: selectedProjectKey,
        query: searchQuery,
        statusFilter,
      }),
    [rawProjects, searchQuery, selectedProjectKey, statusFilter],
  );

  const focusUnexploredEntries = useMemo(() => {
    if (statusFilter === "completed") return [];
    const scoped = filterRecentProjects(rawProjects, {
      projectKey: selectedProjectKey,
      query: searchQuery,
      statusFilter: "unexplored",
    });
    return extractUnexploredEntries(scoped);
  }, [rawProjects, searchQuery, selectedProjectKey, statusFilter]);

  const hasActiveFilters =
    statusFilter !== "all" ||
    selectedProjectKey !== "all" ||
    searchQuery.trim().length > 0;

  function clearFilters() {
    setStatusFilter("all");
    setSelectedProjectKey("all");
    setSearchQuery("");
  }

  return (
    <div
      className="flex min-h-0 flex-1 flex-col gap-3"
      data-testid="memory-recent-workspace"
    >
      <DataToolbar
        actions={
          <ToolbarCluster
            ariaLabel={t("memory.recent.toolbarLabel")}
            className="flex-wrap justify-end"
          >
            <DebouncedToolbarSearch
              className="w-64 min-w-[14rem]"
              commitDelayMs={150}
              commitImmediatelyWhen={(next) => next.trim().length === 0}
              onChange={setSearchQuery}
              placeholder={t("memory.recent.searchPlaceholder")}
              value={searchQuery}
            />
            <ToolbarActionButton
              disabled={reslicing || loading}
              icon={
                <RefreshCw
                  className={reslicing ? "animate-spin" : undefined}
                  size={15}
                />
              }
              label={t("memory.recent.resliceTitle")}
              onClick={() => void handleReslice()}
              text={t("memory.recent.reslice")}
            />
            <ToolbarActionButton
              disabled={isMemoryTaskRunning}
              icon={
                <Sparkles
                  className={isMemoryTaskRunning ? "animate-pulse" : undefined}
                  size={15}
                />
              }
              label={
                isMemoryTaskRunning
                  ? t("memory.recent.generating")
                  : t("memory.recent.triggerGenerate")
              }
              onClick={() => void handleGenerateNow()}
              primary
              text={
                isMemoryTaskRunning
                  ? t("memory.recent.generating")
                  : t("memory.recent.triggerGenerate")
              }
            />
          </ToolbarCluster>
        }
        ariaLabel={t("memory.recent.toolbarLabel")}
        leading={
          <ToolbarCluster
            ariaLabel={t("memory.recent.viewToggleLabel")}
            className="flex-wrap"
          >
            <PillTabs
              activeId={view}
              ariaLabel={t("memory.recent.viewToggleLabel")}
              items={viewTabs}
              onSelect={(id) => setView(id)}
              size="sm"
            />
            <ToolbarSingleSelectDropdown
              ariaLabel={t("memory.recent.filterStatusAll")}
              icon={<Filter size={14} />}
              onChange={setStatusFilter}
              options={statusOptions}
              value={statusFilter}
            />
            <ToolbarSingleSelectDropdown
              ariaLabel={t("memory.recent.filterProjectAria")}
              icon={<FolderOpen size={14} />}
              onChange={setSelectedProjectKey}
              options={projectOptions}
              value={selectedProjectKey}
            />
            <ToolbarSingleSelectDropdown
              ariaLabel={t("memory.recent.windowAria")}
              icon={<Clock3 size={14} />}
              onChange={(nextWindow) => {
                setSelectedWindow(nextWindow);
                void handleReslice(nextWindow);
              }}
              options={windowOptions}
              value={selectedWindow}
            />
          </ToolbarCluster>
        }
      />

      {actionError && (
        <Panel
          className="flex items-center justify-between gap-3 border-status-remove/40"
          padding="sm"
          variant="inset"
        >
          <div className="flex items-center gap-2 text-body-sm text-status-remove">
            <AlertCircle className="shrink-0" size={16} />
            <span>{actionError}</span>
          </div>
          <Button
            aria-label={t("memory.recent.searchClear")}
            onClick={() => setActionError(null)}
            size="icon-sm"
            type="button"
            variant="ghost"
          >
            <X size={14} />
          </Button>
        </Panel>
      )}

      {dismissedUndo && (
        <Panel
          className="flex flex-wrap items-center justify-between gap-3"
          padding="sm"
          variant="inset"
        >
          <span className="min-w-0 flex-1 truncate text-body-sm text-on-surface-variant">
            {t("memory.recent.dismissedNotice", {
              summary: dismissedUndo.summary,
            })}
          </span>
          <div className="flex items-center gap-2">
            <Button
              onClick={() => void handleUndoDismiss()}
              size="sm"
              type="button"
              variant="secondary"
            >
              <RotateCcw size={13} />
              <span>{t("memory.recent.action.restore")}</span>
            </Button>
            <Button
              onClick={() => setDismissedUndo(null)}
              size="icon-sm"
              type="button"
              variant="ghost"
            >
              <X size={14} />
            </Button>
          </div>
        </Panel>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto pr-1">
        {loading ? (
          <AppSkeleton label={t("memory.recent.generating")} layout="list" />
        ) : loadError ? (
          <EmptyState
            actions={
              <Button
                onClick={() => void loadSnapshot()}
                size="sm"
                type="button"
                variant="outline"
              >
                <RefreshCw size={14} />
                <span>{t("memory.recent.refresh")}</span>
              </Button>
            }
            description={loadError}
            icon={<AlertCircle size={20} />}
            title={t("memory.recent.errorTitle")}
          />
        ) : rawProjects.length === 0 ? (
          <EmptyState
            actions={
              <div className="flex flex-wrap items-center justify-center gap-2.5">
                <Button
                  disabled={isMemoryTaskRunning}
                  onClick={() => void handleGenerateNow()}
                  size="default"
                  type="button"
                  variant="default"
                >
                  <Sparkles size={15} />
                  <span>
                    {isMemoryTaskRunning
                      ? t("memory.recent.generating")
                      : t("memory.recent.generateNow")}
                  </span>
                </Button>
                <Button
                  disabled={reslicing}
                  onClick={() => void handleReslice()}
                  size="default"
                  type="button"
                  variant="outline"
                >
                  <RefreshCw size={15} />
                  <span>{t("memory.recent.reslice")}</span>
                </Button>
              </div>
            }
            description={t("memory.recent.emptyDescription")}
            icon={<GitBranch size={20} />}
            title={t("memory.recent.emptyTitle")}
          />
        ) : (
          <div className="flex flex-col gap-5 pb-4">
            {focusUnexploredEntries.length > 0 && (
              <Panel
                data-testid="unexplored-focus-panel"
                padding="none"
                variant="muted"
              >
                <header className="flex flex-wrap items-center justify-between gap-2 border-b border-theme-card-border/65 px-5 py-3.5">
                  <div className="flex min-w-0 items-center gap-2.5">
                    <span className="grid size-8 shrink-0 place-items-center rounded-xl border border-status-conflict/35 bg-status-conflict/12 text-status-conflict">
                      <GitBranch size={15} />
                    </span>
                    <div className="min-w-0">
                      <div className="flex flex-wrap items-center gap-2">
                        <h3 className="text-title-sm font-semibold text-on-surface">
                          {t("memory.recent.unexploredFocusTitle")}
                        </h3>
                        <Badge tone="conflict">
                          {focusUnexploredEntries.length}
                        </Badge>
                      </div>
                      <p className="mt-0.5 text-caption text-on-surface-muted">
                        {t("memory.recent.unexploredFocusSubtitle")}
                      </p>
                    </div>
                  </div>
                  <Button
                    aria-expanded={focusExpanded}
                    onClick={() => setFocusExpanded((prev) => !prev)}
                    size="sm"
                    type="button"
                    variant="ghost"
                  >
                    {focusExpanded ? (
                      <ChevronUp size={15} />
                    ) : (
                      <ChevronDown size={15} />
                    )}
                  </Button>
                </header>

                {focusExpanded && (
                  <div className="grid grid-cols-1 gap-3 p-4 xl:grid-cols-2">
                    {focusUnexploredEntries.map((entry) => (
                      <MemoryNodeCard
                        contextLabel={`${entry.projectTitle} · ${entry.topicName}`}
                        highlightUnexplored
                        key={`focus-${entry.projectKey}-${entry.node.id}`}
                        node={entry.node}
                        onNavigateSession={onNavigateSession}
                        onStatusChange={(targetNode, nextStatus) =>
                          void handleStatusChange(
                            targetNode,
                            nextStatus,
                            entry.projectKey,
                          )
                        }
                        showParentContext
                        t={t}
                        updating={updatingNodeId === entry.node.id}
                      />
                    ))}
                  </div>
                )}
              </Panel>
            )}

            {filteredProjects.length === 0 ? (
              <EmptyState
                actions={
                  hasActiveFilters ? (
                    <Button
                      onClick={clearFilters}
                      size="sm"
                      type="button"
                      variant="outline"
                    >
                      <span>{t("memory.recent.searchClear")}</span>
                    </Button>
                  ) : undefined
                }
                description={t("memory.recent.filteredEmptyDescription")}
                icon={<Filter size={20} />}
                title={t("memory.recent.filteredEmptyTitle")}
              />
            ) : view === "project" ? (
              <RecentTopicTree
                expandedHistorySlugs={expandedHistorySlugs}
                fullProjectStates={fullProjectStates}
                loadingProjectSlugs={loadingProjectSlugs}
                onNavigateSession={onNavigateSession}
                onStatusChange={(targetNode, nextStatus, projectKey) =>
                  void handleStatusChange(targetNode, nextStatus, projectKey)
                }
                onToggleProjectHistory={(projectKey) =>
                  void handleToggleProjectHistory(projectKey)
                }
                projects={filteredProjects}
                searchQuery={searchQuery}
                statusFilter={statusFilter}
                t={t}
                updatingNodeId={updatingNodeId}
              />
            ) : (
              <MemoryDateRailView
                onNavigateSession={onNavigateSession}
                onStatusChange={(targetNode, nextStatus, projectKey) =>
                  void handleStatusChange(targetNode, nextStatus, projectKey)
                }
                projects={filteredProjects}
                t={t}
                updatingNodeId={updatingNodeId}
              />
            )}
          </div>
        )}
      </div>
    </div>
  );
}
