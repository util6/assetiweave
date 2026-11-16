import type { Translator } from "../../../i18n/I18nProvider";
import type {
  MemoryNavigationTarget,
  MemoryNodeStatus,
  MemoryProjectStateView,
  RecentMemoryStateView,
  RecentProjectView,
  RecentTopicView,
  RecentTreeNodeView,
} from "../../../types/memory";

export interface NodeAnchor {
  shortId: string;
  turnNumber: number;
  subIndex?: number;
}

export type TreeHierarchyNode = RecentTreeNodeView & {
  children: TreeHierarchyNode[];
  depth: number;
};

export type RecentStatusFilter = "all" | "unexplored" | "completed";
export type RecentViewMode = "project" | "time";

export interface RecentMetricsSummary {
  projectCount: number;
  topicCount: number;
  nodeCount: number;
  unexploredCount: number;
  windowHours: number;
}

export interface UnexploredBranchEntry {
  node: RecentTreeNodeView;
  projectKey: string;
  projectTitle: string;
  projectPath: string | null;
  topicId: string;
  topicName: string;
}

export interface DateRailProjectGroup {
  projectKey: string;
  projectTitle: string;
  projectPath: string | null;
  entries: Array<{
    node: RecentTreeNodeView;
    topicId: string;
    topicName: string;
    recordedAt: string;
  }>;
}

export interface DateRailDayGroup {
  dateKey: string;
  dateTitle: string;
  relativeLabel: string;
  totalNodes: number;
  projects: DateRailProjectGroup[];
}

export function parseNodeAnchor(nodeId: string): NodeAnchor | null {
  const trimmed = nodeId.trim();
  const match = /^([^#]+)#T(\d+)(?:\.(\d+))?$/.exec(trimmed);
  if (!match) return null;
  return {
    shortId: match[1],
    turnNumber: Number.parseInt(match[2], 10),
    subIndex:
      match[3] !== undefined ? Number.parseInt(match[3], 10) : undefined,
  };
}

export function buildTreeHierarchy(
  nodes: RecentTreeNodeView[],
): TreeHierarchyNode[] {
  const nodeMap = new Map<string, RecentTreeNodeView>();
  for (const node of nodes) {
    nodeMap.set(node.id, node);
  }

  const childrenMap = new Map<string, RecentTreeNodeView[]>();
  const visiting = new Set<string>();
  const visited = new Set<string>();
  const brokenCycleToRoot = new Set<string>();

  function dfsDetectCycle(id: string) {
    if (visited.has(id)) return;
    visiting.add(id);

    const node = nodeMap.get(id);
    if (node?.parentId && nodeMap.has(node.parentId)) {
      if (visiting.has(node.parentId)) {
        brokenCycleToRoot.add(id);
      } else {
        dfsDetectCycle(node.parentId);
      }
    }

    visiting.delete(id);
    visited.add(id);
  }

  for (const node of nodes) {
    dfsDetectCycle(node.id);
  }

  const roots: RecentTreeNodeView[] = [];
  for (const node of nodes) {
    const parentId = node.parentId;
    if (parentId && nodeMap.has(parentId) && !brokenCycleToRoot.has(node.id)) {
      const list = childrenMap.get(parentId) ?? [];
      list.push(node);
      childrenMap.set(parentId, list);
    } else {
      roots.push(node);
    }
  }

  const sortByTime = (a: RecentTreeNodeView, b: RecentTreeNodeView) =>
    (a.recordedAt ?? "").localeCompare(b.recordedAt ?? "");

  roots.sort(sortByTime);

  const built = new Set<string>();

  function toHierarchy(
    node: RecentTreeNodeView,
    depth: number,
  ): TreeHierarchyNode {
    built.add(node.id);
    const rawChildren = childrenMap.get(node.id) ?? [];
    rawChildren.sort(sortByTime);
    const children: TreeHierarchyNode[] = [];
    for (const child of rawChildren) {
      if (!built.has(child.id)) {
        children.push(toHierarchy(child, depth + 1));
      }
    }
    return { ...node, children, depth };
  }

  return roots.map((root) => toHierarchy(root, 0));
}

export function getStatusBadgeTone(
  status: MemoryNodeStatus,
): "create" | "conflict" | "update" | "neutral" {
  switch (status) {
    case "completed":
      return "create";
    case "unexplored":
      return "conflict";
    case "continued":
      return "update";
    case "dismissed":
      return "neutral";
  }
}

export function buildNodeNavigationTarget(
  node: RecentTreeNodeView,
): MemoryNavigationTarget | null {
  const parsed = parseNodeAnchor(node.id);
  const sessionId = node.sourceSessionId?.trim() || parsed?.shortId;
  if (!sessionId) return null;

  const turnStr =
    node.turnIndex != null
      ? String(node.turnIndex)
      : parsed
        ? String(parsed.turnNumber)
        : null;

  return {
    record_kind: "session",
    source_id: null,
    session_id: sessionId,
    question_id: null,
    part_id: null,
    turn_id: turnStr,
    block_id: turnStr,
  };
}

export function computeRecentMetrics(
  stateView: RecentMemoryStateView | null,
): RecentMetricsSummary {
  const snapshot = stateView?.snapshot ?? null;
  const projects = snapshot?.projects ?? [];
  let topicCount = 0;
  let nodeCount = 0;
  let unexploredCount = 0;

  for (const project of projects) {
    const topics = project.topics ?? [];
    topicCount += topics.length;
    for (const topic of topics) {
      nodeCount += topic.nodes.length;
      for (const node of topic.nodes) {
        if (node.status === "unexplored") {
          unexploredCount += 1;
        }
      }
    }
  }

  return {
    projectCount: projects.length,
    topicCount,
    nodeCount,
    unexploredCount,
    windowHours: snapshot?.windowHours ?? 72,
  };
}

export function filterRecentProjects(
  projects: RecentProjectView[],
  {
    projectKey,
    query,
    statusFilter,
  }: {
    projectKey: string;
    query: string;
    statusFilter: RecentStatusFilter;
  },
): RecentProjectView[] {
  const normalizedQuery = query.trim().toLowerCase();

  const nextProjects: RecentProjectView[] = [];
  for (const project of projects) {
    if (projectKey !== "all" && project.projectKey !== projectKey) {
      continue;
    }

    const projectMatches =
      normalizedQuery.length > 0 &&
      [
        project.projectTitle,
        project.projectKey,
        project.projectPath ?? "",
      ].some((field) => field.toLowerCase().includes(normalizedQuery));

    const nextTopics: RecentTopicView[] = [];
    for (const topic of project.topics ?? []) {
      const topicMatches =
        normalizedQuery.length > 0 &&
        topic.topic.toLowerCase().includes(normalizedQuery);

      const nextNodes = topic.nodes.filter((node) => {
        if (statusFilter !== "all" && node.status !== statusFilter) {
          return false;
        }
        if (normalizedQuery.length === 0 || projectMatches || topicMatches) {
          return true;
        }
        return [
          node.id,
          node.label,
          node.summary,
          node.parentSummary ?? "",
          node.sessionTitle ?? "",
          node.sourceAgent ?? "",
          node.sourceSessionId ?? "",
        ].some((field) => field.toLowerCase().includes(normalizedQuery));
      });

      if (nextNodes.length > 0) {
        nextTopics.push({
          ...topic,
          nodes: nextNodes,
        });
      }
    }

    if (nextTopics.length > 0) {
      nextProjects.push({
        ...project,
        topics: nextTopics,
      });
    }
  }

  return nextProjects;
}

export function extractUnexploredEntries(
  projects: RecentProjectView[],
): UnexploredBranchEntry[] {
  const entries: UnexploredBranchEntry[] = [];
  for (const project of projects) {
    for (const topic of project.topics ?? []) {
      for (const node of topic.nodes) {
        if (node.status === "unexplored") {
          entries.push({
            node,
            projectKey: project.projectKey,
            projectTitle: project.projectTitle,
            projectPath: project.projectPath,
            topicId: topic.topicId,
            topicName: topic.topic,
          });
        }
      }
    }
  }
  entries.sort((a, b) =>
    (b.node.recordedAt ?? "").localeCompare(a.node.recordedAt ?? ""),
  );
  return entries;
}

export function projectStateToRecentProjectView(
  fullState: MemoryProjectStateView,
  recentProject?: RecentProjectView | null,
): RecentProjectView & { watermarkCount: number } {
  const recentNodeMap = new Map<string, RecentTreeNodeView>();
  if (recentProject) {
    for (const topic of recentProject.topics ?? []) {
      for (const node of topic.nodes) {
        recentNodeMap.set(node.id, node);
      }
    }
  }

  const allNodesMap = new Map<string, RecentTreeNodeView>();
  for (const topic of fullState.topics) {
    for (const node of topic.nodes) {
      allNodesMap.set(node.id, node);
    }
  }

  const topics: RecentTopicView[] = fullState.topics
    .map((topic) => {
      const nodes: RecentTreeNodeView[] = topic.nodes.map((node) => {
        const cached = recentNodeMap.get(node.id);
        const parentSummary =
          node.parentSummary ??
          cached?.parentSummary ??
          (node.parentId ? (allNodesMap.get(node.parentId)?.summary ?? null) : null);

        return {
          ...node,
          recordedAt: node.recordedAt ?? cached?.recordedAt ?? topic.updatedAt,
          sourceSessionId: node.sourceSessionId ?? cached?.sourceSessionId ?? null,
          turnIndex: node.turnIndex ?? cached?.turnIndex ?? null,
          sessionTitle: node.sessionTitle ?? cached?.sessionTitle ?? null,
          sourceAgent: node.sourceAgent ?? cached?.sourceAgent ?? null,
          sessionAvailable: node.sessionAvailable ?? cached?.sessionAvailable ?? null,
          parentSummary,
        };
      });

      nodes.sort((a, b) =>
        (a.recordedAt ?? "").localeCompare(b.recordedAt ?? ""),
      );
      return {
        ...topic,
        nodes,
      };
    })
    .filter((topic) => topic.nodes.length > 0);

  topics.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));

  return {
    projectKey: fullState.projectKey,
    projectTitle: recentProject?.projectTitle ?? fullState.projectTitle,
    projectPath: fullState.projectPath ?? recentProject?.projectPath ?? null,
    summary: recentProject?.summary ?? "",
    noMaterialChange: recentProject?.noMaterialChange ?? false,
    latestActivityAt: recentProject?.latestActivityAt ?? fullState.updatedAt,
    sourceSessionCount: recentProject?.sourceSessionCount ?? 0,
    items: recentProject?.items ?? [],
    topics,
    watermarkCount: fullState.watermarkCount,
  };
}

export function groupProjectsByDateRail(
  projects: RecentProjectView[],
  t: Translator,
): DateRailDayGroup[] {
  const byDate = new Map<
    string,
    Map<
      string,
      {
        projectKey: string;
        projectTitle: string;
        projectPath: string | null;
        entries: Array<{
          node: RecentTreeNodeView;
          topicId: string;
          topicName: string;
          recordedAt: string;
        }>;
      }
    >
  >();

  for (const project of projects) {
    for (const topic of project.topics ?? []) {
      for (const node of topic.nodes) {
        const effectiveTime =
          node.recordedAt ?? topic.updatedAt ?? project.latestActivityAt;
        const dateKey = extractDateKey(effectiveTime);
        let projectMap = byDate.get(dateKey);
        if (!projectMap) {
          projectMap = new Map();
          byDate.set(dateKey, projectMap);
        }
        let group = projectMap.get(project.projectKey);
        if (!group) {
          group = {
            projectKey: project.projectKey,
            projectTitle: project.projectTitle,
            projectPath: project.projectPath,
            entries: [],
          };
          projectMap.set(project.projectKey, group);
        }
        group.entries.push({
          node,
          topicId: topic.topicId,
          topicName: topic.topic,
          recordedAt: effectiveTime,
        });
      }
    }
  }

  return Array.from(byDate.entries())
    .sort(([a], [b]) => b.localeCompare(a))
    .map(([dateKey, projectMap]) => {
      const { title, relative } = parseDateRailParts(dateKey, t);
      const dayProjects = Array.from(projectMap.values())
        .map((group) => ({
          ...group,
          entries: [...group.entries].sort((a, b) =>
            b.recordedAt.localeCompare(a.recordedAt),
          ),
        }))
        .sort((a, b) => {
          const latestA = a.entries[0]?.recordedAt ?? "";
          const latestB = b.entries[0]?.recordedAt ?? "";
          return latestB.localeCompare(latestA);
        });

      const totalNodes = dayProjects.reduce(
        (sum, group) => sum + group.entries.length,
        0,
      );

      return {
        dateKey,
        dateTitle: title,
        relativeLabel: relative,
        totalNodes,
        projects: dayProjects,
      };
    });
}

export function extractDateKey(isoString: string): string {
  const parsed = new Date(isoString);
  if (Number.isNaN(parsed.getTime())) {
    const prefix = isoString.slice(0, 10);
    return /^\d{4}-\d{2}-\d{2}$/.test(prefix) ? prefix : "unknown";
  }
  const year = parsed.getFullYear();
  const month = String(parsed.getMonth() + 1).padStart(2, "0");
  const day = String(parsed.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

export function formatShortTime(value: string | null | undefined): string {
  if (!value) return "";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value.slice(0, 16).replace("T", " ");
  }
  return new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    month: "2-digit",
    day: "2-digit",
  }).format(parsed);
}

export function formatClockTime(value: string | null | undefined): string {
  if (!value) return "";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value.slice(11, 16) || value;
  }
  return new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  }).format(parsed);
}

export function parseDateRailParts(
  dateKey: string,
  t: Translator,
): {
  title: string;
  relative: string;
} {
  const isZh = t("memory.recent.today") === "今天";
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(dateKey);
  if (!match) {
    return { title: dateKey, relative: "" };
  }
  const y = Number.parseInt(match[1], 10);
  const m = Number.parseInt(match[2], 10);
  const d = Number.parseInt(match[3], 10);
  const target = new Date(y, m - 1, d);
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const diffDays = Math.round(
    (today.getTime() - target.getTime()) / (1000 * 60 * 60 * 24),
  );

  let relative = "";
  if (diffDays === 0) {
    relative = t("memory.recent.today");
  } else if (diffDays === 1) {
    relative = t("memory.recent.yesterday");
  } else if (diffDays === 2) {
    relative = t("memory.recent.beforeYesterday");
  } else if (diffDays > 2) {
    relative = t("memory.recent.daysAgo", { days: diffDays });
  }

  const title = isZh
    ? `${m}月${d}日`
    : new Intl.DateTimeFormat("en-US", {
        month: "short",
        day: "numeric",
      }).format(target);

  return { title, relative };
}
