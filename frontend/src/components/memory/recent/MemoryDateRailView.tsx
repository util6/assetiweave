import { FolderOpen } from "lucide-react";
import { useMemo } from "react";
import type { Translator } from "../../../i18n/I18nProvider";
import type {
  MemoryNavigationTarget,
  MemoryNodeStatus,
  RecentProjectView,
  RecentTreeNodeView,
} from "../../../types/memory";
import { Badge } from "../../foundation/Badge";
import { Panel } from "../../foundation/Panel";
import { MemoryNodeCard } from "./MemoryNodeCard";
import { groupProjectsByDateRail } from "./recentModel";

export interface MemoryDateRailViewProps {
  onNavigateSession?: (target: MemoryNavigationTarget) => void;
  onStatusChange?: (
    node: RecentTreeNodeView,
    nextStatus: MemoryNodeStatus,
    projectKey: string,
  ) => void;
  projects: RecentProjectView[];
  t: Translator;
  updatingNodeId?: string | null;
}

export function MemoryDateRailView({
  onNavigateSession,
  onStatusChange,
  projects,
  t,
  updatingNodeId,
}: MemoryDateRailViewProps) {
  const days = useMemo(
    () => groupProjectsByDateRail(projects, t),
    [projects, t],
  );

  return (
    <div className="flex flex-col gap-6" data-testid="memory-date-rail-view">
      {days.map((day) => (
        <section
          className="grid grid-cols-1 gap-4 lg:grid-cols-[8.75rem_minmax(0,1fr)]"
          data-date-key={day.dateKey}
          key={day.dateKey}
        >
          <aside className="flex flex-row items-baseline justify-between gap-2 border-b border-theme-card-border/50 pb-2 lg:sticky lg:top-0 lg:flex-col lg:items-start lg:justify-start lg:self-start lg:border-b-0 lg:pb-0">
            <div className="flex flex-wrap items-baseline gap-2 lg:flex-col lg:gap-0.5">
              {day.relativeLabel && (
                <span className="text-caption font-semibold uppercase tracking-wide text-primary">
                  {day.relativeLabel}
                </span>
              )}
              <h3 className="text-title-sm font-semibold text-on-surface">
                {day.dateTitle}
              </h3>
            </div>
            <Badge tone="neutral">
              {t("memory.recent.nodeCountBadge", { count: day.totalNodes })}
            </Badge>
          </aside>

          <div className="flex flex-col gap-3.5">
            {day.projects.map((projectGroup) => (
              <Panel
                className="overflow-hidden"
                key={`${day.dateKey}-${projectGroup.projectKey}`}
                padding="none"
                variant="default"
              >
                <header className="flex flex-wrap items-center justify-between gap-2 border-b border-theme-card-border/70 bg-theme-card-header/40 px-4 py-3">
                  <div className="flex min-w-0 items-center gap-2">
                    <span className="grid size-7 shrink-0 place-items-center rounded-xl border border-theme-control-border/70 bg-theme-control/70 text-primary">
                      <FolderOpen size={14} />
                    </span>
                    <h4 className="truncate text-body-sm font-semibold text-on-surface">
                      {projectGroup.projectTitle}
                    </h4>
                  </div>
                  <Badge tone="neutral">
                    {t("memory.recent.nodeCountBadge", {
                      count: projectGroup.entries.length,
                    })}
                  </Badge>
                </header>

                <div className="flex flex-col gap-2.5 p-4">
                  {projectGroup.entries.map((entry) => (
                    <MemoryNodeCard
                      compactTime
                      contextLabel={entry.topicName}
                      highlightUnexplored
                      key={entry.node.id}
                      node={entry.node}
                      onNavigateSession={onNavigateSession}
                      onStatusChange={
                        onStatusChange
                          ? (targetNode, nextStatus) =>
                              onStatusChange(
                                targetNode,
                                nextStatus,
                                projectGroup.projectKey,
                              )
                          : undefined
                      }
                      showParentContext
                      t={t}
                      updating={updatingNodeId === entry.node.id}
                    />
                  ))}
                </div>
              </Panel>
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}
