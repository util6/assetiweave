import { PageMetrics } from "../common/PageMetrics";
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
    <PageMetrics
      metrics={[
        { label: t("source.metric.total"), value: total },
        { label: t("source.metric.enabled"), value: enabled },
        { label: t("source.metric.assets"), value: assets },
        { label: t("source.metric.issues"), value: issues },
      ]}
    />
  );
}
