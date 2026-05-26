import { useI18n } from "../../i18n/I18nProvider";
import type { AppOverview, Asset, DeploymentPlan, ExecutionResult } from "../../types";
import { Metric } from "../common/Metric";

export function DashboardMetrics({
  overview,
  assets,
  plan,
  executionResult,
}: {
  overview: AppOverview | null;
  assets: Asset[];
  plan: DeploymentPlan | null;
  executionResult: ExecutionResult | null;
}) {
  const { t } = useI18n();

  return (
    <>
      <div className="grid grid-cols-4 gap-3">
        <Metric label={t("metric.sources")} value={overview?.source_count ?? 0} />
        <Metric label={t("metric.assets")} value={overview?.asset_count ?? assets.length} />
        <Metric label={t("metric.profiles")} value={overview?.profile_count ?? 0} />
        <Metric label={t("metric.plan")} value={plan ? t("plan.createSummary", { count: plan.summary.create_count }) : t("plan.notGenerated")} />
      </div>

      {plan && (
        <div className="grid grid-cols-5 gap-3">
          <Metric label={t("metric.create")} value={plan.summary.create_count} />
          <Metric label={t("metric.update")} value={plan.summary.update_count} />
          <Metric label={t("metric.remove")} value={plan.summary.remove_count} />
          <Metric label={t("metric.skip")} value={plan.summary.skip_count} />
          <Metric label={t("metric.conflict")} value={plan.summary.conflict_count} />
        </div>
      )}

      {executionResult && (
        <div className="grid grid-cols-4 gap-3">
          <Metric label={t("metric.executed")} value={executionResult.executed_count} />
          <Metric label={t("metric.execSkip")} value={executionResult.skipped_count} />
          <Metric label={t("metric.execConflict")} value={executionResult.conflict_count} />
          <Metric label={t("metric.errors")} value={executionResult.errors.length} />
        </div>
      )}
    </>
  );
}
