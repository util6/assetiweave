import { Save, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Switch } from "../ui/switch";
import { useI18n } from "../../i18n/I18nProvider";
import type { Asset, AssetGroup, AssetGroupDetail } from "../../types";
import { groupMemberAssetIds } from "../../utils/skillGroups";
import { AssetPickerText, DialogFrame, Field } from "./SkillGroupCreateDialog";

interface SkillGroupEditDialogProps {
  assets: Asset[];
  busy: boolean;
  detail: AssetGroupDetail | null;
  onClose: () => void;
  onSubmit: (group: AssetGroup, manualAssetIds: string[]) => Promise<void>;
}

export function SkillGroupEditDialog({
  assets,
  busy,
  detail,
  onClose,
  onSubmit,
}: SkillGroupEditDialogProps) {
  const { t } = useI18n();
  const [name, setName] = useState(detail?.group.name ?? "");
  const [description, setDescription] = useState(detail?.group.description ?? "");
  const [color, setColor] = useState(detail?.group.color ?? "#10b981");
  const [enabled, setEnabled] = useState(detail?.group.enabled ?? true);
  const [query, setQuery] = useState("");
  const [manualAssetIds, setManualAssetIds] = useState<Set<string>>(() => new Set(detail?.manual_asset_ids ?? []));
  const [formError, setFormError] = useState<string | null>(null);

  const skillAssets = useMemo(() => assets.filter((asset) => asset.kind === "skill"), [assets]);
  const filteredAssets = useMemo(() => filterAssets(skillAssets, query), [query, skillAssets]);
  const ruleAssetIds = useMemo(
    () =>
      new Set(
        detail?.members
          .filter((member) => member.origin === "rule" || member.origin === "manual_and_rule")
          .map((member) => member.asset_id) ?? [],
      ),
    [detail],
  );
  const selectedCount = useMemo(
    () => new Set([...groupMemberAssetIds(detail), ...manualAssetIds]).size,
    [detail, manualAssetIds],
  );

  useEffect(() => {
    setName(detail?.group.name ?? "");
    setDescription(detail?.group.description ?? "");
    setColor(detail?.group.color ?? "#10b981");
    setEnabled(detail?.group.enabled ?? true);
    setManualAssetIds(new Set(detail?.manual_asset_ids ?? []));
    setQuery("");
    setFormError(null);
  }, [detail]);

  if (!detail) {
    return null;
  }

  async function handleSubmit() {
    if (!detail) {
      return;
    }

    const trimmedName = name.trim();
    if (!trimmedName) {
      setFormError(t("group.form.error.nameRequired"));
      return;
    }

    setFormError(null);
    await onSubmit(
      {
        ...detail.group,
        name: trimmedName,
        description: description.trim() || null,
        color,
        enabled,
      },
      [...manualAssetIds],
    );
  }

  function toggleManualAsset(assetId: string) {
    setManualAssetIds((current) => {
      const next = new Set(current);
      if (next.has(assetId)) {
        next.delete(assetId);
      } else {
        next.add(assetId);
      }
      return next;
    });
  }

  return (
    <DialogFrame busy={busy} onClose={onClose} title={t("group.editDialog.title")}>
      <div className="grid gap-4">
        <section className="grid gap-3 rounded-xl border border-border bg-surface-lowest/35 p-3">
          <Field label={t("group.field.name")}>
            <Input disabled={busy} onChange={(event) => setName(event.target.value)} value={name} />
          </Field>
          <Field label={t("group.field.description")}>
            <textarea
              className="min-h-20 resize-y rounded-lg border border-border bg-surface-high px-3 py-2 text-body-sm text-on-surface outline-none transition-colors placeholder:text-outline focus:border-primary-strong/60 disabled:cursor-not-allowed disabled:opacity-50"
              disabled={busy}
              onChange={(event) => setDescription(event.target.value)}
              value={description}
            />
          </Field>
          <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 max-[720px]:grid-cols-1">
            <Field label={t("group.field.color")}>
              <input
                className="h-10 w-full rounded-lg border border-border bg-surface-high px-2"
                disabled={busy}
                onChange={(event) => setColor(event.target.value)}
                type="color"
                value={color}
              />
            </Field>
            <label className="mt-6 flex h-10 items-center gap-2 rounded-lg border border-border bg-surface-high px-3 max-[720px]:mt-0">
              <Switch checked={enabled} disabled={busy} onCheckedChange={setEnabled} />
              <span className="text-body-sm text-on-surface-variant">{t("group.field.enabled")}</span>
            </label>
          </div>
          {formError && <div className="text-body-sm text-status-remove">{formError}</div>}
        </section>

        <section className="grid gap-3 rounded-xl border border-border bg-surface-lowest/35 p-3">
          <div className="grid gap-3">
            <div className="flex items-center justify-between gap-3 max-[720px]:flex-col max-[720px]:items-stretch">
              <div className="min-w-0">
                <div className="text-label-caps uppercase text-outline">{t("group.editDialog.assets")}</div>
                <div className="mt-1 text-body-sm text-on-surface-variant">
                  {t("group.assets.selected", { selected: selectedCount, total: skillAssets.length })}
                </div>
              </div>
            </div>
            <label className="flex h-10 min-w-0 items-center gap-2 rounded-xl border border-border bg-surface-high/90 px-3 text-outline transition-colors focus-within:border-primary/60 focus-within:text-primary">
              <Search size={16} />
              <input
                className="min-w-0 flex-1 border-0 bg-transparent text-body-sm text-on-surface outline-none placeholder:text-outline"
                onChange={(event) => setQuery(event.target.value)}
                placeholder={t("group.search.skills")}
                value={query}
              />
            </label>
          </div>

          <div className="max-h-[360px] overflow-y-auto rounded-xl border border-border bg-surface-card/35">
            {filteredAssets.length === 0 ? (
              <div className="px-4 py-5 text-body-sm text-on-surface-variant">{t("group.assets.empty")}</div>
            ) : (
              filteredAssets.map((asset) => {
                const ruleMatched = ruleAssetIds.has(asset.id);
                const selected = manualAssetIds.has(asset.id) || ruleMatched;
                return (
                  <label
                    className="grid min-h-[74px] cursor-pointer grid-cols-[auto_minmax(0,1fr)] items-center gap-3 border-b border-border/70 px-4 py-3 text-left last:border-b-0 hover:bg-surface-low/70 has-[:disabled]:cursor-not-allowed has-[:disabled]:opacity-60"
                    key={asset.id}
                  >
                    <input
                      checked={selected}
                      className="size-4 rounded border-border accent-primary"
                      disabled={busy || ruleMatched}
                      onChange={() => toggleManualAsset(asset.id)}
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
            <Save size={16} />
            {t("group.editDialog.submit")}
          </Button>
        </div>
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
