import { RefreshCw, Search } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";

export function SourceToolbar({
  busy,
  query,
  onQueryChange,
  onScan,
}: {
  busy: boolean;
  query: string;
  onQueryChange: (query: string) => void;
  onScan: () => void;
}) {
  const { t } = useI18n();

  return (
    <div className="flex items-center justify-between gap-3 max-[920px]:flex-col max-[920px]:items-stretch">
      <label className="flex h-10 min-w-72 flex-1 items-center gap-2 rounded-xl border border-border bg-surface-high px-3 text-outline focus-within:border-primary/50">
        <Search size={17} />
        <input
          className="min-w-0 flex-1 border-0 bg-transparent text-body-sm text-on-surface outline-none placeholder:text-outline"
          onChange={(event) => onQueryChange(event.target.value)}
          placeholder={t("source.toolbar.searchPlaceholder")}
          value={query}
        />
      </label>

      <div className="flex items-center justify-end gap-2">
        <button
          className="inline-flex h-10 items-center justify-center gap-2 rounded-xl border border-border bg-surface-high px-3 text-body-sm text-on-surface-variant transition-colors hover:bg-surface-highest hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-50"
          disabled={busy}
          onClick={onScan}
          type="button"
        >
          <RefreshCw size={17} />
          <span>{t("source.toolbar.scanAll")}</span>
        </button>
      </div>
    </div>
  );
}
