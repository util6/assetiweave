import { useI18n } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import type { Asset } from "../../types";

export function SkillBackupBadge({ asset }: { asset: Asset }) {
  const { t } = useI18n();
  const state = asset.backup_status?.state;
  if (!state) {
    return null;
  }

  const labelKey = (state === "downloaded" ? "backup.badge.downloaded" : "backup.badge.backedUp") as TranslationKey;
  return (
    <span className="shrink-0 rounded-md border border-status-update/25 bg-status-update/10 px-2 py-0.5 text-label-caps uppercase text-status-update">
      {t(labelKey)}
    </span>
  );
}
