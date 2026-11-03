import {
  ArrowRightCircle,
  CheckCircle2,
  ExternalLink,
  FolderOpen,
  Sparkles,
} from "lucide-react";
import { useCallback } from "react";
import type { Translator } from "../../i18n/I18nProvider";
import type {
  MemoryNavigationTarget,
  RecentNodeStatus,
  RecentProjectView,
  RecentTopicView,
  RecentTreeNodeView,
} from "../../types/memory";
import { Panel } from "../foundation/Panel";

export interface NodeAnchor {
  shortId: string;
  turnNumber: number;
  subIndex?: number;
}

export function parseNodeAnchor(nodeId: string): NodeAnchor | null {
  const match = nodeId.trim().match(/^([a-zA-Z0-9_-]+)#T(\d+)(?:\.(\d+))?$/);
  if (!match) return null;
  return {
    shortId: match[1],
    turnNumber: parseInt(match[2], 10),
    subIndex: match[3] ? parseInt(match[3], 10) : undefined,
  };
}

export interface HierarchyNode extends RecentTreeNodeView {
  children: HierarchyNode[];
}

export function buildTreeHierarchy(nodes: RecentTreeNodeView[]): HierarchyNode[] {
  const nodeMap = new Map<string, HierarchyNode>();
  for (const n of nodes) {
    nodeMap.set(n.id, { ...n, children: [] });
  }

  const roots: HierarchyNode[] = [];
  for (const n of nodes) {
    const current = nodeMap.get(n.id)!;
    if (n.parentId && nodeMap.has(n.parentId)) {
      nodeMap.get(n.parentId)!.children.push(current);
    } else {
      roots.push(current);
    }
  }

  return roots;
}

export interface RecentTopicTreeProps {
  project: RecentProjectView;
  onNavigateSession?: (target: MemoryNavigationTarget) => void;
  t: Translator;
}

export function RecentTopicTree({
  project,
  onNavigateSession,
  t,
}: RecentTopicTreeProps) {
  const handleJump = useCallback(
    (nodeId: string) => {
      const parsed = parseNodeAnchor(nodeId);
      if (!parsed || !onNavigateSession) return;
      onNavigateSession({
        record_kind: "session",
        source_id: null,
        session_id: parsed.shortId,
        question_id: null,
        turn_id: String(parsed.turnNumber),
        part_id: null,
        block_id: String(parsed.turnNumber),
      });
    },
    [onNavigateSession],
  );

  const topics = project.topics ?? [];

  return (
    <Panel
      className="flex flex-col gap-5 p-4.5 rounded-2xl border border-theme-control-border/70 bg-surface/85 backdrop-blur-md shadow-xs"
      variant="default"
    >
      {/* Project Header */}
      <div className="flex items-center justify-between gap-3 border-b border-theme-control-border/60 pb-3">
        <div className="flex items-center gap-2.5 min-w-0">
          <span className="grid size-7 shrink-0 place-items-center rounded-xl border border-primary/20 bg-primary/10 text-primary shadow-xs">
            <FolderOpen size={14} />
          </span>
          <span className="truncate text-body-md font-bold text-on-surface">
            {project.projectTitle}
          </span>
          {project.projectPath && (
            <span className="hidden sm:inline-block max-w-[280px] truncate rounded-full border border-theme-control-border/60 bg-theme-control/30 px-2.5 py-0.5 font-mono text-[11px] text-on-surface-variant">
              {project.projectPath}
            </span>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <span className="rounded-full bg-primary/10 px-2.5 py-0.5 text-[11px] font-semibold text-primary">
            {topics.length} {t("memory.recent.topics") || "个主题"}
          </span>
        </div>
      </div>

      {/* Topics */}
      <div className="flex flex-col gap-6">
        {topics.map((topic) => (
          <TopicSection
            handleJump={handleJump}
            key={topic.topicId}
            t={t}
            topic={topic}
          />
        ))}
      </div>
    </Panel>
  );
}

interface TopicSectionProps {
  topic: RecentTopicView;
  handleJump: (nodeId: string) => void;
  t: Translator;
}

function TopicSection({ topic, handleJump, t }: TopicSectionProps) {
  const roots = buildTreeHierarchy(topic.nodes);

  // Status summaries
  const completedCount = topic.nodes.filter(
    (n) => n.status === "completed",
  ).length;
  const continuedCount = topic.nodes.filter(
    (n) => n.status === "continued",
  ).length;
  const unexploredCount = topic.nodes.filter(
    (n) => n.status === "unexplored",
  ).length;

  return (
    <div className="flex flex-col gap-3 rounded-2xl border border-theme-control-border/50 bg-theme-control/10 p-3.5 sm:p-4">
      {/* Topic Header */}
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-theme-control-border/40 pb-2.5">
        <div className="flex items-center gap-2">
          <span className="rounded-full border border-theme-control-border/80 bg-theme-control/60 px-2.5 py-0.5 font-mono text-[11px] font-bold text-primary">
            {topic.topicId}
          </span>
          <span className="text-body-sm font-bold text-on-surface">
            {topic.topic}
          </span>
        </div>

        {/* Status Chips */}
        <div className="flex items-center gap-1.5 flex-wrap">
          {completedCount > 0 && (
            <span className="inline-flex items-center gap-1 rounded-full border border-status-add/30 bg-status-add/10 px-2 py-0.5 text-[10px] font-medium text-status-add">
              <CheckCircle2 size={11} className="shrink-0" />
              <span>{completedCount} 已完成</span>
            </span>
          )}
          {continuedCount > 0 && (
            <span className="inline-flex items-center gap-1 rounded-full border border-primary/30 bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary">
              <ArrowRightCircle size={11} className="shrink-0" />
              <span>{continuedCount} 推进中</span>
            </span>
          )}
          {unexploredCount > 0 && (
            <span className="inline-flex items-center gap-1 rounded-full border border-status-issue/30 bg-status-issue/10 px-2 py-0.5 text-[10px] font-medium text-status-issue">
              <Sparkles size={11} className="shrink-0" />
              <span>{unexploredCount} 待探索</span>
            </span>
          )}
        </div>
      </div>

      {/* Tree Nodes */}
      <div className="flex flex-col gap-2 pt-1">
        {roots.map((rootNode) => (
          <TreeNodeCard
            depth={0}
            handleJump={handleJump}
            key={rootNode.id}
            node={rootNode}
            t={t}
          />
        ))}
      </div>
    </div>
  );
}

interface TreeNodeCardProps {
  node: HierarchyNode;
  depth: number;
  handleJump: (nodeId: string) => void;
  t: Translator;
}

function TreeNodeCard({ node, depth, handleJump }: TreeNodeCardProps) {
  const parsed = parseNodeAnchor(node.id);

  return (
    <div className="relative flex flex-col">
      {/* Node content card */}
      <div className="group relative flex flex-col gap-1.5 rounded-xl border border-theme-control-border/60 bg-surface/70 backdrop-blur-xs p-3 transition-colors hover:border-theme-control-border hover:bg-surface/95 shadow-2xs">
        {/* Top line: status badge, lineage notes, and jump button */}
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2 flex-wrap">
            <NodeStatusBadge status={node.status} />
            {node.parentId && (
              <span className="font-mono text-[10px] text-on-surface-variant/80">
                (↖ 继承自 {node.parentId})
              </span>
            )}
          </div>

          {parsed && (
            <button
              className="inline-flex items-center gap-1 rounded-full border border-theme-control-border bg-theme-control/80 px-2 py-0.5 font-mono text-[11px] text-primary hover:border-primary/50 hover:bg-theme-control cursor-pointer transition-colors shadow-2xs"
              onClick={() => handleJump(node.id)}
              title={`跳转到会话轮次 ${parsed.shortId} #T${parsed.turnNumber}`}
              type="button"
            >
              <span>
                {parsed.shortId} #T{parsed.turnNumber}
              </span>
              <ExternalLink className="shrink-0" size={10} />
            </button>
          )}
        </div>

        {/* Node label and summary */}
        <div className="flex flex-col gap-0.5">
          {node.label && (
            <span className="text-body-sm font-semibold text-on-surface">
              {node.label}
            </span>
          )}
          {node.summary && node.summary !== node.label && (
            <span className="text-body-xs text-on-surface-variant leading-relaxed">
              {node.summary}
            </span>
          )}
        </div>
      </div>

      {/* Children with visual connector lines */}
      {node.children.length > 0 && (
        <div className="relative ml-5 sm:ml-6 mt-2 flex flex-col gap-2 pl-4 border-l-2 border-theme-control-border/60">
          {node.children.map((child) => (
            <div className="relative" key={child.id}>
              {/* Elbow connector curve */}
              <div className="absolute -left-[18px] top-4 h-3 w-3.5 rounded-bl-xl border-b-2 border-l-2 border-theme-control-border/60 pointer-events-none" />
              <TreeNodeCard
                depth={depth + 1}
                handleJump={handleJump}
                node={child}
                t={TranslatorDefault}
              />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

const TranslatorDefault: Translator = (k: string) => k;

function NodeStatusBadge({ status }: { status: RecentNodeStatus }) {
  switch (status) {
    case "completed":
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-status-add/40 bg-status-add/10 px-2 py-0.5 text-[11px] font-medium text-status-add">
          <CheckCircle2 className="shrink-0" size={12} />
          <span>已完成</span>
        </span>
      );
    case "continued":
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-primary/40 bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary">
          <ArrowRightCircle className="shrink-0" size={12} />
          <span>推进中</span>
        </span>
      );
    case "unexplored":
      return (
        <span className="inline-flex items-center gap-1 rounded-full border border-status-issue/40 bg-status-issue/10 px-2 py-0.5 text-[11px] font-medium text-status-issue">
          <Sparkles className="shrink-0" size={12} />
          <span>待探索</span>
        </span>
      );
  }
}
