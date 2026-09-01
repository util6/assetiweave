import { Brain } from "lucide-react";
import type { ReactNode } from "react";
import { MemoryRecallWorkspace } from "../../components/memory/MemoryRecallWorkspace";
import { MemoryRecentWorkspace } from "../../components/memory/MemoryRecentWorkspace";
import { PageHeader } from "../../components/foundation/PageHeader";
import { useI18n } from "../../i18n/I18nProvider";
import { getMemoryRecentEventTarget } from "../../services/memory";
import type { MemoryNavigationTarget, RecentMemoryEvent } from "../../types/memory";

export function MemoryPage({
  activeSubNavId,
  onNavigate,
}: {
  activeSubNavId: string;
  onNavigate?: (target: MemoryNavigationTarget) => void;
}) {
  const { t } = useI18n();
  const isRecall = activeSubNavId === "recall";
  return (
    <MemoryWorkspacePage description={t(isRecall ? "memory.recall.description" : "memory.recent.description")} title={t(isRecall ? "memory.recall.title" : "memory.recent.title")}>
      {isRecall ? (
        <MemoryRecallWorkspace onNavigate={onNavigate} t={t} />
      ) : (
        <MemoryRecentWorkspace onEventOpen={(event) => void openRecentEvent(event, onNavigate)} t={t} />
      )}
    </MemoryWorkspacePage>
  );
}

async function openRecentEvent(event: RecentMemoryEvent, onNavigate?: (target: MemoryNavigationTarget) => void) {
  if (!onNavigate) return;
  const target = await getMemoryRecentEventTarget(event.id);
  if (!target) return;
  onNavigate({
    record_kind: target.record_kind,
    source_id: null,
    session_id: target.session_id,
    question_id: target.question_id,
    turn_id: target.turn_id,
    part_id: null,
    block_id: target.block_id,
  });
}

function MemoryWorkspacePage({ children, description, title }: { children: ReactNode; description: string; title: string }) {
  const { t } = useI18n();
  return (
    <section className="flex min-h-0 flex-1 flex-col gap-[var(--app-section-gap)] overflow-hidden px-[var(--app-page-x)] py-[var(--app-page-y)]">
      <PageHeader description={description} eyebrow={t("memory.page.eyebrow")} icon={<Brain size={16} />} title={title} />
      {children}
    </section>
  );
}
