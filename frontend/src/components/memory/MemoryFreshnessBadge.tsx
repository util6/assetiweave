import { AlertTriangle, CircleCheck, Link2Off } from "lucide-react";
import type { MemoryStaleReason } from "../../types/memory";
import { useI18n } from "../../i18n/I18nProvider";

export function MemoryFreshnessBadge({ reason }: { reason: MemoryStaleReason | null }) {
  const { t } = useI18n();
  if (!reason) {
    return <span className="inline-flex items-center gap-1 rounded-full bg-status-create/10 px-2 py-1 text-label-sm text-status-create"><CircleCheck size={13} />{t("memory.freshness.verified")}</span>;
  }
  const unavailable = reason === "source_unavailable";
  const label = reason === "evidence_changed" ? t("memory.freshness.changed") : reason === "evidence_missing" ? t("memory.freshness.missing") : t("memory.freshness.unavailable");
  return <span className="inline-flex items-center gap-1 rounded-full bg-status-warning/10 px-2 py-1 text-label-sm text-status-warning">{unavailable ? <Link2Off size={13} /> : <AlertTriangle size={13} />}{label}</span>;
}
