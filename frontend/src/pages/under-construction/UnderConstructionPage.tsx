import { UnderConstructionState } from "../../components/foundation/UnderConstructionState";
import { useI18n } from "../../i18n/I18nProvider";

export function UnderConstructionPage({
  featureLabel,
  routeKey,
}: {
  featureLabel?: string;
  routeKey?: string;
}) {
  const { t } = useI18n();
  const feature = featureLabel?.trim() || t("underConstruction.defaultFeature");

  return (
    <section className="flex flex-1 flex-col gap-[var(--app-section-gap)] px-[var(--app-page-x)] py-[var(--app-page-y)]">
      <UnderConstructionState
        actions={
          routeKey ? (
            <span className="rounded-md border border-theme-control-border bg-theme-control px-2 py-1 font-mono text-code-md text-on-surface-variant">
              {t("underConstruction.routeKey", { routeKey })}
            </span>
          ) : undefined
        }
        description={t("underConstruction.description")}
        eyebrow={t("underConstruction.eyebrow")}
        title={t("underConstruction.title", { feature })}
      />
    </section>
  );
}
