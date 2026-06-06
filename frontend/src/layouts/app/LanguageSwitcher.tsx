import clsx from "clsx";
import { Languages } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";

export function LanguageSwitcher() {
  const { locale, setLocale, t } = useI18n();

  return (
    <div
      className="flex h-9 shrink-0 items-center gap-1 rounded-xl border border-theme-control-border bg-theme-control p-1 text-body-sm"
      aria-label={t("language.label")}
      role="group"
    >
      <Languages size={16} className="mx-1 text-outline" aria-hidden="true" />
      {(["zh", "en"] as const).map((nextLocale) => (
        <button
          className={clsx(
            "h-7 rounded-lg px-2.5 font-semibold transition-colors",
            locale === nextLocale ? "bg-theme-control-hover text-primary" : "text-theme-control-fg hover:text-on-surface",
          )}
          key={nextLocale}
          onClick={() => setLocale(nextLocale)}
          type="button"
        >
          {t(nextLocale === "zh" ? "language.zh" : "language.en")}
        </button>
      ))}
    </div>
  );
}
