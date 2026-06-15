import { FolderPlus } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Switch } from "../ui/switch";
import { DialogFrame } from "../foundation/DialogFrame";
import { useI18n } from "../../i18n/I18nProvider";
import { DEFAULT_GROUP_COLOR_HEX } from "../../theme/themes";
import type { Asset, AssetGroupInput } from "../../types";
import { AssetPickerHeader, AssetPickerText, GroupField } from "./SkillGroupFormPrimitives";

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
  const nameInputRef = useRef<HTMLInputElement>(null);
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

  const footer = (
    <div className="flex items-center justify-end gap-2">
      <Button disabled={busy} onClick={onClose} type="button" variant="outline">
        {t("group.dialog.cancel")}
      </Button>
      <Button disabled={busy} onClick={() => void handleSubmit()} type="button">
        {t("group.createDialog.submit")}
      </Button>
    </div>
  );

  return (
    <DialogFrame
      busy={busy}
      closeLabel={t("group.dialog.close")}
      footer={footer}
      icon={<FolderPlus size={18} />}
      iconClassName="border-status-create/25 bg-status-create/15 text-status-create"
      initialFocusRef={nameInputRef}
      onClose={onClose}
      size="xl"
      title={t("group.createDialog.title")}
    >
      <div className="grid gap-4">
        <section className="grid gap-2 rounded-xl border border-theme-card-border bg-theme-card/65 p-3">
          <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] items-end gap-2 max-[720px]:grid-cols-1">
            <GroupField label={t("group.field.name")}>
              <Input disabled={busy} onChange={(event) => setName(event.target.value)} ref={nameInputRef} value={name} />
            </GroupField>
            <GroupField label={t("group.field.description")}>
              <Input disabled={busy} onChange={(event) => setDescription(event.target.value)} value={description} />
            </GroupField>
          </div>
          <div className="grid grid-cols-[auto_1fr_auto] items-center gap-2 max-[720px]:grid-cols-1">
            <GroupField label={t("group.field.color")}>
              <input
                className="h-8 w-16 rounded-lg border border-theme-control-border bg-theme-control px-1"
                disabled={busy}
                onChange={(event) => setColor(event.target.value)}
                type="color"
                value={color}
              />
            </GroupField>
            <div />
            <label className="flex h-8 items-center gap-2 self-end rounded-lg border border-theme-control-border bg-theme-control px-3">
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
      </div>
    </DialogFrame>
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
