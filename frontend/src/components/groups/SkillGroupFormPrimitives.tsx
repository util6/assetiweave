import { CheckSquare } from "lucide-react";
import type { ReactNode } from "react";
import { DebouncedToolbarSearch } from "../common/DataToolbar";
import { useI18n } from "../../i18n/I18nProvider";
import type { Asset } from "../../types";
import { displayAssetPath } from "../../utils/path";

const ASSET_PICKER_SEARCH_COMMIT_DELAY_MS = 700;

export function GroupField({ children, label }: { children: ReactNode; label: string }) {
  return (
    <label className="grid gap-1.5">
      <span className="text-body-sm font-medium text-on-surface-variant">{label}</span>
      {children}
    </label>
  );
}

export function AssetPickerText({ asset }: { asset: Asset }) {
  return (
    <span className="min-w-0">
      <span className="block overflow-hidden text-ellipsis whitespace-nowrap font-mono text-code-md font-semibold text-on-surface">
        {asset.name}
      </span>
      <span className="mt-1 block overflow-hidden text-ellipsis whitespace-nowrap font-mono text-body-sm text-on-surface-variant">
        {displayAssetPath(asset)}
      </span>
    </span>
  );
}

export function AssetPickerHeader({
  onQueryChange,
  onToggleAll,
  query,
  selectedCount,
  title,
  totalCount,
}: {
  onQueryChange: (query: string) => void;
  onToggleAll?: () => void;
  query: string;
  selectedCount: number;
  title: string;
  totalCount: number;
}) {
  const { t } = useI18n();

  return (
    <div className="grid gap-3">
      <div className="flex items-center justify-between gap-3 max-[720px]:flex-col max-[720px]:items-stretch">
        <div className="min-w-0">
          <div className="text-label-caps uppercase text-outline">{title}</div>
          <div className="mt-1 text-body-sm text-on-surface-variant">
            {t("group.assets.selected", { selected: selectedCount, total: totalCount })}
          </div>
        </div>
        {onToggleAll && (
          <button
            className="inline-flex h-9 items-center justify-center gap-2 rounded-lg border border-theme-control-border bg-theme-control px-3 text-body-sm font-semibold text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-on-surface"
            onClick={onToggleAll}
            type="button"
          >
            <CheckSquare size={16} />
            {t("group.assets.toggleVisible")}
          </button>
        )}
      </div>
      <DebouncedToolbarSearch
        className="w-full min-w-0"
        commitDelayMs={ASSET_PICKER_SEARCH_COMMIT_DELAY_MS}
        onChange={onQueryChange}
        placeholder={t("group.search.skills")}
        submitLabel={t("group.search.submitSkills")}
        value={query}
      />
    </div>
  );
}
