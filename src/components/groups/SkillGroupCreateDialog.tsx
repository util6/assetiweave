import { CheckSquare, Search, X } from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { DialogFrame as FoundationDialogFrame } from "../foundation/DialogFrame";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Switch } from "../ui/switch";
import { useI18n } from "../../i18n/I18nProvider";
import { cn } from "../../lib/utils";
import { DEFAULT_GROUP_COLOR_HEX } from "../../theme/themes";
import { iconButtonRecipe } from "../../theme/recipes";
import type { Asset, AssetGroupInput } from "../../types";
import { displayAssetPath } from "../../utils/path";

interface SkillGroupCreateDialogProps {
  assets: Asset[];
  busy: boolean;
  nextSortOrder: number;
  onClose: () => void;
  onSubmit: (input: AssetGroupInput, assetIds: string[]) => Promise<void>;
  open: boolean;
}

export function SkillGroupCreateDialog({
  assets,
  busy,
  nextSortOrder,
  onClose,
  onSubmit,
  open,
}: SkillGroupCreateDialogProps) {
  const { t } = useI18n();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [color, setColor] = useState(DEFAULT_GROUP_COLOR_HEX);
  const [enabled, setEnabled] = useState(true);
  const [query, setQuery] = useState("");
  const [selectedAssetIds, setSelectedAssetIds] = useState<Set<string>>(new Set());
  const [formError, setFormError] = useState<string | null>(null);

  const skillAssets = useMemo(() => assets.filter((asset) => asset.kind === "skill"), [assets]);
  const filteredAssets = useMemo(() => filterAssets(skillAssets, query), [query, skillAssets]);
  const selectedCount = selectedAssetIds.size;

  useEffect(() => {
    if (!open) {
      return;
    }

    setName("");
    setDescription("");
    setColor(DEFAULT_GROUP_COLOR_HEX);
    setEnabled(true);
    setQuery("");
    setSelectedAssetIds(new Set());
    setFormError(null);
  }, [open]);

  if (!open) {
    return null;
  }

  async function handleSubmit() {
    const trimmedName = name.trim();
    if (!trimmedName) {
      setFormError(t("group.form.error.nameRequired"));
      return;
    }

    setFormError(null);
    await onSubmit(
      {
        name: trimmedName,
        description: description.trim() || null,
        color,
        enabled,
        sort_order: nextSortOrder,
        rules: { source_ids: [], relative_path_globs: [], name_contains: null },
      },
      [...selectedAssetIds],
    );
  }

  function toggleAsset(assetId: string) {
    setSelectedAssetIds((current) => {
      const next = new Set(current);
      if (next.has(assetId)) {
        next.delete(assetId);
      } else {
        next.add(assetId);
      }
      return next;
    });
  }

  function toggleAllVisible() {
    setSelectedAssetIds((current) => {
      const next = new Set(current);
      const allVisibleSelected = filteredAssets.length > 0 && filteredAssets.every((asset) => next.has(asset.id));
      for (const asset of filteredAssets) {
        if (allVisibleSelected) {
          next.delete(asset.id);
        } else {
          next.add(asset.id);
        }
      }
      return next;
    });
  }

  return (
    <DialogFrame
      busy={busy}
      onClose={onClose}
      title={t("group.createDialog.title")}
    >
      <div className="grid gap-4">
        <section className="grid gap-3 rounded-xl border border-theme-card-border bg-theme-card/65 p-3">
          <Field label={t("group.field.name")}>
            <Input disabled={busy} onChange={(event) => setName(event.target.value)} value={name} />
          </Field>
          <Field label={t("group.field.description")}>
            <textarea
              className="min-h-20 resize-y rounded-lg border border-theme-control-border bg-theme-control px-3 py-2 text-body-sm text-on-surface outline-none transition-colors placeholder:text-outline focus:border-primary-strong/60 disabled:cursor-not-allowed disabled:opacity-50"
              disabled={busy}
              onChange={(event) => setDescription(event.target.value)}
              value={description}
            />
          </Field>
          <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 max-[720px]:grid-cols-1">
            <Field label={t("group.field.color")}>
              <input
                className="h-10 w-full rounded-lg border border-theme-control-border bg-theme-control px-2"
                disabled={busy}
                onChange={(event) => setColor(event.target.value)}
                type="color"
                value={color}
              />
            </Field>
            <label className="mt-6 flex h-10 items-center gap-2 rounded-lg border border-theme-control-border bg-theme-control px-3 max-[720px]:mt-0">
              <Switch checked={enabled} disabled={busy} onCheckedChange={setEnabled} />
              <span className="text-body-sm text-on-surface-variant">{t("group.field.enabled")}</span>
            </label>
          </div>
          {formError && <div className="text-body-sm text-status-remove">{formError}</div>}
        </section>

        <section className="grid gap-3 rounded-xl border border-theme-card-border bg-theme-card/65 p-3">
          <AssetPickerHeader
            onQueryChange={setQuery}
            onToggleAll={toggleAllVisible}
            query={query}
            selectedCount={selectedCount}
            title={t("group.createDialog.assets")}
            totalCount={skillAssets.length}
          />
          <div className="max-h-[340px] overflow-y-auto rounded-xl border border-theme-card-border bg-theme-card/45">
            {filteredAssets.length === 0 ? (
              <div className="px-4 py-5 text-body-sm text-on-surface-variant">{t("group.assets.empty")}</div>
            ) : (
              filteredAssets.map((asset) => {
                const selected = selectedAssetIds.has(asset.id);
                return (
                  <label
                    className="grid min-h-[74px] cursor-pointer grid-cols-[auto_minmax(0,1fr)] items-center gap-3 border-b border-theme-card-border px-4 py-3 text-left last:border-b-0 hover:bg-theme-card-header/70 has-[:disabled]:cursor-not-allowed has-[:disabled]:opacity-60"
                    key={asset.id}
                  >
                    <input
                      checked={selected}
                      className="size-4 rounded border-theme-control-border accent-primary"
                      disabled={busy}
                      onChange={() => toggleAsset(asset.id)}
                      type="checkbox"
                    />
                    <AssetPickerText asset={asset} />
                  </label>
                );
              })
            )}
          </div>
        </section>

        <div className="flex items-center justify-end gap-2">
          <Button disabled={busy} onClick={onClose} type="button" variant="outline">
            {t("group.dialog.cancel")}
          </Button>
          <Button disabled={busy} onClick={() => void handleSubmit()} type="button">
            {t("group.createDialog.submit")}
          </Button>
        </div>
      </div>
    </DialogFrame>
  );
}

export function DialogFrame({
  busy,
  children,
  onClose,
  title,
}: {
  busy: boolean;
  children: ReactNode;
  onClose: () => void;
  title: string;
}) {
  const { t } = useI18n();

  return (
    <FoundationDialogFrame
      className="flex max-h-[92vh] max-w-4xl flex-col"
      contentClassName="min-h-0 overflow-y-auto p-4"
      headerActions={
        <button
          aria-label={t("group.dialog.close")}
          className={cn(iconButtonRecipe({ size: "sm" }))}
          disabled={busy}
          onClick={onClose}
          title={t("group.dialog.close")}
          type="button"
        >
          <X size={17} />
        </button>
      }
      onBackdropClick={busy ? undefined : onClose}
      title={title}
    >
      {children}
    </FoundationDialogFrame>
  );
}

export function Field({ children, label }: { children: ReactNode; label: string }) {
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

function AssetPickerHeader({
  onQueryChange,
  onToggleAll,
  query,
  selectedCount,
  title,
  totalCount,
}: {
  onQueryChange: (query: string) => void;
  onToggleAll: () => void;
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
        <button
          className="inline-flex h-9 items-center justify-center gap-2 rounded-lg border border-theme-control-border bg-theme-control px-3 text-body-sm font-semibold text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-on-surface"
          onClick={onToggleAll}
          type="button"
        >
          <CheckSquare size={16} />
          {t("group.assets.toggleVisible")}
        </button>
      </div>
      <label className="flex h-10 min-w-0 items-center gap-2 rounded-xl border border-theme-control-border bg-theme-control/90 px-3 text-outline transition-colors focus-within:border-primary/60 focus-within:text-primary">
        <Search size={16} />
        <input
          className="min-w-0 flex-1 border-0 bg-transparent text-body-sm text-on-surface outline-none placeholder:text-outline"
          onChange={(event) => onQueryChange(event.target.value)}
          placeholder={t("group.search.skills")}
          value={query}
        />
      </label>
    </div>
  );
}

function filterAssets(assets: Asset[], query: string) {
  const normalizedQuery = query.trim().toLowerCase();
  if (!normalizedQuery) {
    return assets;
  }

  return assets.filter((asset) =>
    [asset.name, asset.description ?? "", asset.relative_path].join(" ").toLowerCase().includes(normalizedQuery),
  );
}
