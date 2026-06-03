import { deploymentActionLabel, translatePlanReason } from "../../i18n/domain";
import { useI18n } from "../../i18n/I18nProvider";
import type { DeploymentPlan } from "../../types";
import { planActionClass } from "../../utils/styles";

export function DeploymentPlanPanel({ plan }: { plan: DeploymentPlan | null }) {
  const { t } = useI18n();

  if (!plan) {
    return null;
  }

  return (
    <div className="glass-card overflow-hidden rounded-xl border border-theme-card-border">
      <div className="flex items-center justify-between border-b border-theme-card-border px-4 py-3">
        <span className="text-label-caps uppercase text-outline">{t("plan.title")}</span>
        <span className="font-mono text-body-sm text-primary">{t("plan.actions", { count: plan.actions.length })}</span>
      </div>
      <div className="max-h-56 overflow-y-auto">
        {plan.actions.slice(0, 16).map((action) => (
          <div className="grid grid-cols-[96px_120px_1fr] gap-3 border-b border-theme-card-border px-4 py-2.5 last:border-b-0" key={action.id}>
            <span className={planActionClass(action.action_type)}>{deploymentActionLabel(action.action_type, t)}</span>
            <span className="font-mono text-body-sm text-on-surface-variant">{action.profile_id}</span>
            <div className="min-w-0">
              <p className="overflow-hidden text-ellipsis whitespace-nowrap font-mono text-body-sm text-on-surface">{action.target_path}</p>
              <p className="mt-1 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm text-outline">{translatePlanReason(action.reason, t)}</p>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
