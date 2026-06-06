import { Metric } from "../common/Metric";
import { useI18n } from "../../i18n/I18nProvider";

export function SourceSummary({
  assets,
  enabled,
  issues,
  total,
}: {
  assets: number;
  enabled: number;
  issues: number;
  total: number;
}) {
  const { t } = useI18n();

  return (
    <div className="grid grid-cols-4 gap-3 max-[1180px]:grid-cols-2">
      <Metric label={t("source.metric.total")} value={total} />
      <Metric label={t("source.metric.enabled")} value={enabled} />
      <Metric label={t("source.metric.assets")} value={assets} />
      <Metric label={t("source.metric.issues")} value={issues} />
    </div>
  );
}
