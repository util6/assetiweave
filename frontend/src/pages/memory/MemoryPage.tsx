import { Brain } from "lucide-react";
import { useState, type ReactNode } from "react";
import {
  CollapsibleHeader,
  useCollapsibleHeaderState,
} from "../../components/common/CollapsibleHeader";
import { PageMetrics } from "../../components/common/PageMetrics";
import { MemoryRecallWorkspace } from "../../components/memory/MemoryRecallWorkspace";
import { MemoryRecentWorkspace } from "../../components/memory/MemoryRecentWorkspace";
import type { RecentMetricsSummary } from "../../components/memory/recent/recentModel";
import { PageHeader } from "../../components/foundation/PageHeader";
import { useI18n } from "../../i18n/I18nProvider";
import type { MemoryNavigationTarget } from "../../types/memory";

export function MemoryPage({
  activeSubNavId,
  onNavigate,
}: {
  activeSubNavId: string;
  onNavigate?: (target: MemoryNavigationTarget) => void;
}) {
  const { t } = useI18n();
  const isRecall = activeSubNavId === "recall";
  const [recentMetrics, setRecentMetrics] =
    useState<RecentMetricsSummary | null>(null);

  const headerMetrics =
    !isRecall && recentMetrics ? (
      <PageMetrics
        metrics={[
          {
            label: t("memory.recent.metricProjects"),
            value: recentMetrics.projectCount,
          },
          {
            label: t("memory.recent.metricTopics"),
            value: recentMetrics.topicCount,
          },
          {
            label: t("memory.recent.metricUnexplored"),
            value: recentMetrics.unexploredCount,
          },
          {
            label: t("memory.recent.metricWindow"),
            value: t("memory.recent.metricHoursUnit", {
              hours: recentMetrics.windowHours,
            }),
          },
        ]}
      />
    ) : undefined;

  return (
    <MemoryWorkspacePage
      actions={headerMetrics}
      description={t(
        isRecall ? "memory.recall.description" : "memory.recent.description",
      )}
      title={t(isRecall ? "memory.recall.title" : "memory.recent.title")}
    >
      {isRecall ? (
        <MemoryRecallWorkspace onNavigate={onNavigate} t={t} />
      ) : (
        <MemoryRecentWorkspace
          onMetricsChange={setRecentMetrics}
          onNavigateSession={onNavigate}
          t={t}
        />
      )}
    </MemoryWorkspacePage>
  );
}

function MemoryWorkspacePage({
  actions,
  children,
  description,
  title,
}: {
  actions?: ReactNode;
  children: ReactNode;
  description: string;
  title: string;
}) {
  const { t } = useI18n();
  const { visible: headerVisible, onScroll: onHeaderScroll } =
    useCollapsibleHeaderState();
  return (
    <section
      className="flex min-h-0 flex-1 flex-col gap-[var(--app-section-gap)] overflow-hidden px-[var(--app-page-x)] py-[var(--app-page-y)]"
      onScrollCapture={onHeaderScroll}
    >
      <CollapsibleHeader visible={headerVisible}>
        <PageHeader
          actions={actions}
          description={description}
          eyebrow={t("memory.page.eyebrow")}
          icon={<Brain size={16} />}
          title={title}
        />
      </CollapsibleHeader>
      {children}
    </section>
  );
}
