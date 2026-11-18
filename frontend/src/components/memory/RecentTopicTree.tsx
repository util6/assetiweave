import clsx from "clsx";
import {
  ChevronDown,
  ChevronRight,
  FolderOpen,
  GitBranch,
  History,
  Layers,
  Loader2,
} from "lucide-react";
import { useMemo, useState } from "react";
import type { Translator } from "../../i18n/I18nProvider";
import type {
  MemoryNavigationTarget,
  MemoryNodeStatus,
  MemoryProjectStateView,
  RecentProjectView,
  RecentTopicView,
  RecentTreeNodeView,
} from "../../types/memory";
import { Badge } from "../foundation/Badge";
import { Panel } from "../foundation/Panel";
import { TruncatedText } from "../foundation/TruncatedText";
import { Button } from "../ui/button";
import { abbreviateHomePath } from "../../utils/path";
import { MemoryNodeCard } from "./recent/MemoryNodeCard";
import {
  buildTreeHierarchy,
  filterRecentProjects,
  parseNodeAnchor,
  projectStateToRecentProjectView,
  type NodeAnchor,
  type RecentStatusFilter,
  type TreeHierarchyNode,
} from "./recent/recentModel";

export { buildTreeHierarchy, parseNodeAnchor };
export type { NodeAnchor, TreeHierarchyNode, TreeHierarchyNode as HierarchyNode };

export interface RecentTopicTreeProps {
  expandedHistorySlugs?: Set<string>;
  fullProjectStates?: Record<string, MemoryProjectStateView | null>;
  loadingProjectSlugs?: Set<string>;
  onNavigateSession?: (target: MemoryNavigationTarget) => void;
  onStatusChange?: (
    node: RecentTreeNodeView,
    nextStatus: MemoryNodeStatus,
    projectKey: string,
  ) => void;
  onToggleProjectHistory?: (projectKey: string) => void;
  project?: RecentProjectView;
  projects?: RecentProjectView[];
  searchQuery?: string;
  statusFilter?: RecentStatusFilter;
  t: Translator;
  updatingNodeId?: string | null;
}

export function RecentTopicTree({
  expandedHistorySlugs,
  fullProjectStates,
  loadingProjectSlugs,
  onNavigateSession,
  onStatusChange,
  onToggleProjectHistory,
  project,
  projects,
  searchQuery = "",
  statusFilter = "all",
  t,
  updatingNodeId,
}: RecentTopicTreeProps) {
  const effectiveProjects = useMemo<RecentProjectView[]>(() => {
    if (projects) return projects;
    if (project) return [project];
    return [];
  }, [project, projects]);

  return (
    <div className="flex flex-col gap-4" data-testid="recent-topic-tree">
      {effectiveProjects.map((item, index) => (
        <ProjectTopicTreePanel
          expandedHistory={expandedHistorySlugs?.has(item.projectKey) ?? false}
          fullState={fullProjectStates?.[item.projectKey]}
          key={item.projectKey || item.projectTitle || String(index)}
          loadingHistory={loadingProjectSlugs?.has(item.projectKey) ?? false}
          onNavigateSession={onNavigateSession}
          onStatusChange={onStatusChange}
          onToggleProjectHistory={onToggleProjectHistory}
          project={item}
          searchQuery={searchQuery}
          statusFilter={statusFilter}
          t={t}
          updatingNodeId={updatingNodeId}
        />
      ))}
    </div>
  );
}

function ProjectTopicTreePanel({
  expandedHistory,
  fullState,
  loadingHistory,
  onNavigateSession,
  onStatusChange,
  onToggleProjectHistory,
  project,
  searchQuery,
  statusFilter,
  t,
  updatingNodeId,
}: {
  expandedHistory: boolean;
  fullState?: MemoryProjectStateView | null;
  loadingHistory: boolean;
  onNavigateSession?: (target: MemoryNavigationTarget) => void;
  onStatusChange?: (
    node: RecentTreeNodeView,
    nextStatus: MemoryNodeStatus,
    projectKey: string,
  ) => void;
  onToggleProjectHistory?: (projectKey: string) => void;
  project: RecentProjectView;
  searchQuery: string;
  statusFilter: RecentStatusFilter;
  t: Translator;
  updatingNodeId?: string | null;
}) {
  const effectiveProject = useMemo(() => {
    if (!expandedHistory || !fullState) {
      return project;
    }
    const converted = projectStateToRecentProjectView(fullState, project);
    const trimmed = searchQuery.trim();
    if (!trimmed && statusFilter === "all") {
      return converted;
    }
    const filtered = filterRecentProjects([converted], {
      projectKey: "all",
      query: trimmed,
      statusFilter,
    });
    return filtered[0] ?? { ...converted, topics: [] };
  }, [expandedHistory, fullState, project, searchQuery, statusFilter]);

  const effectiveTopics = effectiveProject.topics ?? [];
  const totalNodes = effectiveTopics.reduce(
    (sum, topic) => sum + topic.nodes.length,
    0,
  );
  const unexploredCount = effectiveTopics.reduce(
    (sum, topic) =>
      sum + topic.nodes.filter((node) => node.status === "unexplored").length,
    0,
  );
  const watermarkCount =
    "watermarkCount" in effectiveProject &&
    typeof effectiveProject.watermarkCount === "number"
      ? effectiveProject.watermarkCount
      : fullState?.watermarkCount;

  return (
    <Panel
      className="overflow-hidden"
      data-project-slug={project.projectKey}
      padding="none"
      variant="default"
    >
      <header className="flex flex-wrap items-center justify-between gap-3 border-b border-theme-card-border/75 bg-theme-card-header/45 px-5 py-3.5">
        <div className="flex min-w-0 flex-1 items-center gap-2.5">
          <span className="grid size-8 shrink-0 place-items-center rounded-xl border border-theme-control-border/70 bg-theme-control/75 text-primary">
            <FolderOpen size={15} />
          </span>
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <TruncatedText
                as="h3"
                className="text-title-sm font-semibold text-on-surface"
              >
                {effectiveProject.projectTitle}
              </TruncatedText>
              <Badge tone="neutral">
                {effectiveTopics.length} {t("memory.recent.topics")}
              </Badge>
              <Badge tone="neutral">
                {t("memory.recent.nodeCountBadge", { count: totalNodes })}
              </Badge>
              {unexploredCount > 0 && (
                <Badge tone="conflict">
                  {t("memory.recent.unexploredCountBadge", {
                    count: unexploredCount,
                  })}
                </Badge>
              )}
              {watermarkCount != null && (
                <span className="rounded-full border border-theme-control-border/70 bg-theme-control/60 px-2.5 py-0.5 font-mono text-caption text-on-surface-muted">
                  {t("memory.recent.watermarkSessions", {
                    count: watermarkCount,
                  })}
                </span>
              )}
            </div>
            {effectiveProject.projectPath && (
              <TruncatedText
                as="p"
                className="mt-0.5 font-mono text-caption text-outline"
              >
                {abbreviateHomePath(effectiveProject.projectPath)}
              </TruncatedText>
            )}
          </div>
        </div>

        {onToggleProjectHistory && (
          <Button
            aria-expanded={expandedHistory}
            disabled={loadingHistory}
            onClick={() => onToggleProjectHistory(project.projectKey)}
            size="sm"
            type="button"
            variant={expandedHistory ? "secondary" : "outline"}
          >
            {loadingHistory ? (
              <Loader2 className="animate-spin" size={14} />
            ) : (
              <History size={14} />
            )}
            <span>
              {loadingHistory
                ? t("memory.recent.projectHistoryLoading")
                : expandedHistory
                  ? t("memory.recent.projectHistoryHide")
                  : t("memory.recent.projectHistoryShow")}
            </span>
          </Button>
        )}
      </header>

      {expandedHistory && (
        <div className="border-b border-theme-card-border/60 bg-theme-control/45 px-5 py-2.5 text-caption text-on-surface-variant">
          {loadingHistory ? (
            <span>{t("memory.recent.projectHistoryLoading")}</span>
          ) : fullState ? (
            <span>
              {t("memory.recent.projectHistoryBanner", {
                watermark: fullState.watermarkCount,
              })}
            </span>
          ) : (
            <span>{t("memory.recent.projectHistoryEmpty")}</span>
          )}
        </div>
      )}

      <div className="flex flex-col divide-y divide-theme-card-border/55">
        {effectiveTopics.map((topic) => (
          <TopicBranchSection
            key={topic.topicId}
            onNavigateSession={onNavigateSession}
            onStatusChange={
              onStatusChange
                ? (node, nextStatus) =>
                    onStatusChange(node, nextStatus, project.projectKey)
                : undefined
            }
            t={t}
            topic={topic}
            updatingNodeId={updatingNodeId}
          />
        ))}
      </div>
    </Panel>
  );
}

function TopicBranchSection({
  onNavigateSession,
  onStatusChange,
  t,
  topic,
  updatingNodeId,
}: {
  onNavigateSession?: (target: MemoryNavigationTarget) => void;
  onStatusChange?: (node: RecentTreeNodeView, nextStatus: MemoryNodeStatus) => void;
  t: Translator;
  topic: RecentTopicView;
  updatingNodeId?: string | null;
}) {
  const [collapsed, setCollapsed] = useState(false);
  const hierarchy = useMemo(
    () => buildTreeHierarchy(topic.nodes),
    [topic.nodes],
  );
  const visibleNodeIds = useMemo(
    () => new Set(topic.nodes.map((node) => node.id)),
    [topic.nodes],
  );
  const topicUnexplored = topic.nodes.filter(
    (node) => node.status === "unexplored",
  ).length;

  return (
    <section className="px-5 py-4" data-topic-id={topic.topicId}>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <button
          aria-expanded={!collapsed}
          className="inline-flex min-w-0 items-center gap-2 rounded-xl px-1.5 py-1 text-left transition-colors hover:bg-theme-control-hover/65 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/55"
          onClick={() => setCollapsed((prev) => !prev)}
          type="button"
        >
          <span className="text-outline">
            {collapsed ? <ChevronRight size={15} /> : <ChevronDown size={15} />}
          </span>
          <Layers className="shrink-0 text-primary" size={14} />
          <TruncatedText
            as="h4"
            className="text-body-sm font-semibold text-on-surface"
          >
            {topic.topic}
          </TruncatedText>
        </button>

        <div className="flex items-center gap-1.5">
          {topicUnexplored > 0 && (
            <Badge tone="conflict">
              <GitBranch className="mr-1" size={11} />
              {t("memory.recent.unexploredCountBadge", {
                count: topicUnexplored,
              })}
            </Badge>
          )}
          <span className="font-mono text-caption text-outline">
            {t("memory.recent.nodeCountBadge", { count: topic.nodes.length })}
          </span>
        </div>
      </div>

      {!collapsed && (
        <div className="mt-3 flex flex-col gap-2.5">
          {hierarchy.map((branch) => (
            <TreeBranchNode
              branch={branch}
              key={branch.id}
              onNavigateSession={onNavigateSession}
              onStatusChange={onStatusChange}
              t={t}
              updatingNodeId={updatingNodeId}
              visibleNodeIds={visibleNodeIds}
            />
          ))}
        </div>
      )}
    </section>
  );
}

function TreeBranchNode({
  branch,
  onNavigateSession,
  onStatusChange,
  t,
  updatingNodeId,
  visibleNodeIds,
}: {
  branch: TreeHierarchyNode;
  onNavigateSession?: (target: MemoryNavigationTarget) => void;
  onStatusChange?: (node: RecentTreeNodeView, nextStatus: MemoryNodeStatus) => void;
  t: Translator;
  updatingNodeId?: string | null;
  visibleNodeIds: Set<string>;
}) {
  const { children, depth } = branch;
  const parentMissingFromSlice =
    Boolean(branch.parentId) && !visibleNodeIds.has(branch.parentId!);

  return (
    <div
      className={clsx(
        "flex flex-col gap-2",
        depth > 0 &&
          depth <= 4 &&
          "ml-3 border-l-2 border-theme-card-border/65 pl-4",
      )}
      data-depth={depth}
    >
      <MemoryNodeCard
        highlightUnexplored
        node={branch}
        onNavigateSession={onNavigateSession}
        onStatusChange={onStatusChange}
        showParentContext={parentMissingFromSlice}
        t={t}
        updating={updatingNodeId === branch.id}
      />

      {children.length > 0 && (
        <div className="flex flex-col gap-2">
          {children.map((child) => (
            <TreeBranchNode
              branch={child}
              key={child.id}
              onNavigateSession={onNavigateSession}
              onStatusChange={onStatusChange}
              t={t}
              updatingNodeId={updatingNodeId}
              visibleNodeIds={visibleNodeIds}
            />
          ))}
        </div>
      )}
    </div>
  );
}
