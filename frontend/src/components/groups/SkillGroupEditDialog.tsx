import { Save } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Switch } from "../ui/switch";
import { DialogFrame } from "../foundation/DialogFrame";
import { useI18n } from "../../i18n/I18nProvider";
import { DEFAULT_GROUP_COLOR_HEX } from "../../theme/themes";
import type { Asset, AssetGroup, AssetGroupDetail } from "../../types";
import { groupMemberAssetIds } from "../../utils/skillGroups";
import { AssetPickerHeader, AssetPickerText, GroupField } from "./SkillGroupFormPrimitives";

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
  const nameInputRef = useRef<HTMLInputElement>(null);
  const [name, setName] = useState(detail?.group.name ?? "");
  const [description, setDescription] = useState(detail?.group.description ?? "");
  const [color, setColor] = useState(detail?.group.color ?? DEFAULT_GROUP_COLOR_HEX);
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
    setColor(detail?.group.color ?? DEFAULT_GROUP_COLOR_HEX);
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

  const footer = (
    <div className="flex items-center justify-end gap-2">
      <Button disabled={busy} onClick={onClose} type="button" variant="outline">
        {t("group.dialog.cancel")}
      </Button>
      <Button disabled={busy} onClick={() => void handleSubmit()} type="button">
        <Save size={16} />
        {t("group.editDialog.submit")}
      </Button>
    </div>
  );

  return (
    <DialogFrame
      busy={busy}
      closeLabel={t("group.dialog.close")}
      footer={footer}
      icon={<Save size={18} />}
      iconClassName="border-status-update/25 bg-status-update/15 text-status-update"
      initialFocusRef={nameInputRef}
      onClose={onClose}
      size="xl"
      title={t("group.editDialog.title")}
    >
      <div className="grid gap-4">
        <section className="grid gap-2 rounded-xl border border-theme-card-border bg-theme-card/65 p-3">
          <div className="grid grid-cols-[minmax(0,1fr)_auto] items-end gap-2 max-[720px]:grid-cols-1">
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
            query={query}
            selectedCount={selectedCount}
            title={t("group.editDialog.assets")}
            totalCount={skillAssets.length}
          />

          <div className="max-h-[360px] overflow-y-auto rounded-xl border border-theme-card-border bg-theme-card/45">
            {filteredAssets.length === 0 ? (
              <div className="px-4 py-5 text-body-sm text-on-surface-variant">{t("group.assets.empty")}</div>
            ) : (
              filteredAssets.map((asset) => {
                const ruleMatched = ruleAssetIds.has(asset.id);
                const selected = manualAssetIds.has(asset.id) || ruleMatched;
                return (
                  <label
                    className="grid min-h-[74px] cursor-pointer grid-cols-[auto_minmax(0,1fr)] items-center gap-3 border-b border-theme-card-border px-4 py-3 text-left last:border-b-0 hover:bg-theme-card-header/70 has-[:disabled]:cursor-not-allowed has-[:disabled]:opacity-60"
                    key={asset.id}
                  >
                    <input
                      checked={selected}
                      className="size-4 rounded border-theme-control-border accent-primary"
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
