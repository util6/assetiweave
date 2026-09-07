import { zodResolver } from "@hookform/resolvers/zod";
import { Code2, Save } from "lucide-react";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { Controller, useForm } from "react-hook-form";
import {
  isSkillBackupRunning,
  SkillBackupButtonContent,
  SkillBackupInlineProgress,
} from "../backup/SkillBackupProgress";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Switch } from "../ui/switch";
import { DialogFrame } from "../foundation/DialogFrame";
import { useI18n } from "../../i18n/I18nProvider";
import type { SkillBackupTaskSnapshot } from "../../services/catalog";
import { DEFAULT_GROUP_COLOR_HEX } from "../../theme/themes";
import { isHexColor } from "../../theme/colorValidation";
import type { Asset, AssetGroup, AssetGroupDetail } from "../../types";
import { getBackupableSkillAssetsByIds } from "../../utils/skillBackup";
import {
  AssetPickerHeader,
  AssetPickerText,
  GroupField,
} from "./SkillGroupFormPrimitives";
import {
  type GroupFormValues,
  groupFormSchema,
  groupValuesToInput,
  parseGroupIconSvgInput,
} from "./groupFormSchema";

interface SkillGroupEditDialogProps {
  assets: Asset[];
  backupTask?: SkillBackupTaskSnapshot | null;
  busy: boolean;
  detail: AssetGroupDetail | null;
  onBackup?: (assetIds: string[]) => Promise<void>;
  onClose: () => void;
  onSubmit: (group: AssetGroup, manualAssetIds: string[]) => Promise<void>;
}

export function SkillGroupEditDialog({
  assets,
  backupTask,
  busy,
  detail,
  onBackup,
  onClose,
  onSubmit,
}: SkillGroupEditDialogProps) {
  const { t } = useI18n();
  const formId = useId();
  const nameInputRef = useRef<HTMLInputElement>(null);

  const {
    control,
    formState: { errors },
    handleSubmit,
    register,
    reset,
    setValue,
    watch,
  } = useForm<GroupFormValues>({
    resolver: zodResolver(groupFormSchema),
    defaultValues: {
      name: detail?.group.name ?? "",
      description: detail?.group.description ?? "",
      color: detail?.group.color ?? DEFAULT_GROUP_COLOR_HEX,
      displayIcon: detail?.group.display_icon ?? "",
      iconSvg: detail?.group.icon_svg ?? null,
      enabled: detail?.group.enabled ?? true,
    },
  });

  const [query, setQuery] = useState("");
  const [manualAssetIds, setManualAssetIds] = useState<Set<string>>(
    () => new Set(detail?.manual_asset_ids ?? []),
  );
  const [svgEditorOpen, setSvgEditorOpen] = useState(false);
  const [svgDraft, setSvgDraft] = useState("");
  const [svgError, setSvgError] = useState("");

  const name = watch("name");
  const description = watch("description");
  const color = watch("color");
  const displayIcon = watch("displayIcon");
  const iconSvg = watch("iconSvg");
  const enabled = watch("enabled");

  const skillAssets = useMemo(
    () => assets.filter((asset) => asset.kind === "skill"),
    [assets],
  );
  const skillAssetsById = useMemo(
    () => new Map(skillAssets.map((asset) => [asset.id, asset])),
    [skillAssets],
  );
  const filteredAssets = useMemo(
    () => filterAssets(skillAssets, query),
    [query, skillAssets],
  );
  const ruleAssetIds = useMemo(
    () =>
      new Set(
        detail?.members
          .filter(
            (member) =>
              member.origin === "rule" || member.origin === "manual_and_rule",
          )
          .map((member) => member.asset_id) ?? [],
      ),
    [detail],
  );
  const draftMemberAssetIds = useMemo(() => {
    const assetIds = new Set<string>(ruleAssetIds);
    for (const assetId of manualAssetIds) {
      assetIds.add(assetId);
    }
    return [...assetIds];
  }, [manualAssetIds, ruleAssetIds]);
  const backupAssets = useMemo(
    () => getBackupableSkillAssetsByIds(skillAssetsById, draftMemberAssetIds),
    [draftMemberAssetIds, skillAssetsById],
  );

  useEffect(() => {
    if (!detail) {
      return;
    }
    reset({
      name: detail.group.name ?? "",
      description: detail.group.description ?? "",
      color: detail.group.color ?? DEFAULT_GROUP_COLOR_HEX,
      displayIcon: detail.group.display_icon ?? "",
      iconSvg: detail.group.icon_svg ?? null,
      enabled: detail.group.enabled ?? true,
    });
    setManualAssetIds(new Set(detail.manual_asset_ids ?? []));
    setQuery("");
    setSvgEditorOpen(false);
    setSvgDraft("");
    setSvgError("");
  }, [detail, reset]);

  if (!detail) {
    return null;
  }
  const backupAssetCount = backupAssets.length;
  const backupActionLabel =
    backupAssetCount > 0
      ? t("backup.action.backupCount", { count: backupAssetCount })
      : t("backup.action.allInDirectory");

  function openSvgEditor() {
    setSvgDraft(iconSvg ? JSON.stringify(iconSvg, null, 2) : "");
    setSvgError("");
    setSvgEditorOpen(true);
  }

  function closeSvgEditor() {
    setSvgEditorOpen(false);
    setSvgDraft("");
    setSvgError("");
  }

  function saveIconSvg() {
    const input = svgDraft.trim();
    if (!input) {
      setValue("iconSvg", null, { shouldDirty: true });
      closeSvgEditor();
      return;
    }

    const result = parseGroupIconSvgInput(input);
    if (result) {
      setValue("iconSvg", result, { shouldDirty: true });
      closeSvgEditor();
      return;
    }

    setSvgError(t("group.icon.svgError"));
  }

  function clearIconSvg() {
    setValue("iconSvg", null, { shouldDirty: true });
    closeSvgEditor();
  }

  const onSubmitValid = async (values: GroupFormValues) => {
    if (!detail) {
      return;
    }
    const input = groupValuesToInput(values);
    await onSubmit(
      {
        ...detail.group,
        ...input,
        color: input.color ?? detail.group.color,
      },
      [...manualAssetIds],
    );
  };

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
    <>
      <div className="max-[640px]:grid">
        {onBackup && (
          <>
            <Button
              className="max-[640px]:w-full"
              disabled={
                busy ||
                backupAssetCount === 0 ||
                isSkillBackupRunning(backupTask ?? null)
              }
              onClick={() =>
                void onBackup(backupAssets.map((asset) => asset.id))
              }
              type="button"
              variant="outline"
            >
              <SkillBackupButtonContent
                assetIds={backupAssets.map((asset) => asset.id)}
                defaultLabel={backupActionLabel}
                task={backupTask ?? null}
                t={t}
              />
            </Button>
            <SkillBackupInlineProgress
              assetIds={backupAssets.map((asset) => asset.id)}
              task={backupTask ?? null}
              t={t}
            />
          </>
        )}
      </div>
      <div className="flex items-center justify-end gap-2 max-[640px]:grid max-[640px]:grid-cols-2">
        <Button
          disabled={busy}
          onClick={onClose}
          type="button"
          variant="outline"
        >
          {t("group.dialog.cancel")}
        </Button>
        <Button disabled={busy} form={formId} type="submit">
          <Save size={16} />
          {t("group.editDialog.submit")}
        </Button>
      </div>
    </>
  );

  const { ref: nameFormRef, ...nameRegisterRest } = register("name");

  return (
    <>
      <DialogFrame
        busy={busy}
        closeLabel={t("group.dialog.close")}
        footer={footer}
        footerClassName="justify-between max-[640px]:flex-col max-[640px]:items-stretch"
        icon={<Save size={18} />}
        iconClassName="border-status-update/25 bg-status-update/15 text-status-update"
        initialFocusRef={nameInputRef}
        onClose={onClose}
        size="xl"
        title={t("group.editDialog.title")}
      >
        <form
          className="grid gap-4"
          id={formId}
          onSubmit={handleSubmit(onSubmitValid)}
        >
          <section className="grid gap-4 rounded-xl border border-theme-card-border bg-theme-card/65 p-4">
            <div className="grid grid-cols-[17rem_minmax(0,1fr)] gap-4 max-[760px]:grid-cols-1">
              <div className="grid gap-3 rounded-xl border border-theme-card-border bg-theme-card-header/45 p-4">
                <div className="grid grid-cols-[4rem_minmax(0,1fr)] items-center gap-3">
                  <span
                    aria-hidden="true"
                    className="grid size-16 shrink-0 place-items-center rounded-xl border text-title-md font-bold shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.5)]"
                    style={{
                      borderColor: `${color}66`,
                      backgroundColor: `${color}1f`,
                      color,
                    }}
                  >
                    {iconSvg && iconSvg.paths.length > 0 ? (
                      <svg
                        aria-hidden="true"
                        className="size-8"
                        fill="currentColor"
                        viewBox={iconSvg.view_box ?? "0 0 24 24"}
                      >
                        {iconSvg.paths.map((path, index) => (
                          <path
                            clipRule={path.clip_rule}
                            d={path.d}
                            fillRule={path.fill_rule}
                            key={`${path.d}-${index}`}
                          />
                        ))}
                      </svg>
                    ) : displayIcon.trim() ? (
                      displayIcon.trim().slice(0, 4)
                    ) : (
                      <Save size={24} />
                    )}
                  </span>
                  <div className="min-w-0">
                    <p className="truncate text-title-sm font-bold text-on-surface">
                      {name.trim() || detail.group.name}
                    </p>
                    <p className="mt-1 truncate text-body-sm text-on-surface-variant">
                      {description.trim() ||
                        detail.group.description ||
                        t("group.noDescription")}
                    </p>
                  </div>
                </div>
                <Controller
                  control={control}
                  name="enabled"
                  render={({ field }) => (
                    <label className="flex h-10 w-full items-center justify-between gap-3 rounded-xl border border-theme-control-border bg-theme-control px-3">
                      <Switch
                        checked={field.value}
                        disabled={busy}
                        onCheckedChange={field.onChange}
                      />
                      <span className="whitespace-nowrap text-body-sm text-on-surface-variant">
                        {t("group.field.enabled")}
                      </span>
                    </label>
                  )}
                />
              </div>

              <div className="grid min-w-0 gap-4">
                <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-3 max-[760px]:grid-cols-1">
                  <GroupField
                    error={
                      errors.name
                        ? t("group.form.error.nameRequired")
                        : undefined
                    }
                    label={t("group.field.name")}
                  >
                    <Input
                      {...nameRegisterRest}
                      disabled={busy}
                      ref={(element) => {
                        nameFormRef(element);
                        nameInputRef.current = element;
                      }}
                    />
                  </GroupField>
                  <GroupField label={t("group.field.description")}>
                    <Input {...register("description")} disabled={busy} />
                  </GroupField>
                </div>

                <div className="grid grid-cols-[minmax(0,1fr)_minmax(18rem,1.1fr)] items-end gap-3 max-[900px]:grid-cols-1">
                  <Controller
                    control={control}
                    name="color"
                    render={({ field }) => (
                      <GroupField
                        error={
                          errors.color
                            ? t("group.form.error.colorInvalid")
                            : undefined
                        }
                        label={t("group.field.colorCode")}
                      >
                        <div className="flex h-10 items-center gap-2 rounded-xl border border-theme-control-border bg-theme-control px-2 transition-[background-color,border-color,box-shadow,color] duration-200 focus-within:border-primary-strong/60">
                          <input
                            aria-label={t("group.field.color")}
                            className="size-5 shrink-0 cursor-pointer rounded border-0 bg-transparent p-0"
                            disabled={busy}
                            onChange={(event) => {
                              field.onChange(event.target.value.toLowerCase());
                            }}
                            type="color"
                            value={
                              isHexColor(field.value)
                                ? field.value
                                : DEFAULT_GROUP_COLOR_HEX
                            }
                          />
                          <Input
                            aria-label={t("group.field.colorCode")}
                            className="h-auto min-w-0 flex-1 border-0 bg-transparent p-0 font-mono text-code-md focus:border-transparent"
                            disabled={busy}
                            maxLength={7}
                            onBlur={field.onBlur}
                            onChange={(event) =>
                              field.onChange(event.target.value)
                            }
                            value={field.value}
                          />
                        </div>
                      </GroupField>
                    )}
                  />

                  <GroupField label={t("group.field.icon")}>
                    <div className="flex h-10 min-w-0 items-center gap-2">
                      <Input
                        {...register("displayIcon")}
                        aria-label={t("group.field.iconText")}
                        className="h-10 min-w-0 flex-1 border-theme-control-border bg-theme-control font-mono text-code-md"
                        disabled={busy}
                        maxLength={4}
                        placeholder={t("group.field.iconHint")}
                      />
                      <Button
                        aria-label={t("group.icon.editSvg")}
                        aria-pressed={Boolean(iconSvg)}
                        className="h-10 shrink-0 px-3"
                        disabled={busy}
                        onClick={openSvgEditor}
                        title={t("group.icon.editSvg")}
                        type="button"
                        variant="outline"
                      >
                        <Code2 size={15} />
                        <span>{t("group.field.iconCode")}</span>
                      </Button>
                    </div>
                  </GroupField>
                </div>
              </div>
            </div>
          </section>

          <section className="grid gap-3 rounded-xl border border-theme-card-border bg-theme-card/65 p-3">
            <AssetPickerHeader
              onQueryChange={setQuery}
              query={query}
              selectedCount={draftMemberAssetIds.length}
              title={t("group.editDialog.assets")}
              totalCount={skillAssets.length}
            />

            <div className="grid max-h-72 gap-2 overflow-y-auto">
              {filteredAssets.length === 0 ? (
                <div className="rounded-xl border border-theme-card-border bg-theme-card/35 px-4 py-8 text-center text-body-sm text-on-surface-variant">
                  {t("group.assets.empty")}
                </div>
              ) : (
                filteredAssets.map((asset) => {
                  const isManual = manualAssetIds.has(asset.id);
                  const isRule = ruleAssetIds.has(asset.id);
                  const selected = isManual || isRule;
                  return (
                    <label
                      className="flex cursor-pointer items-center justify-between gap-3 rounded-xl border border-theme-card-border bg-theme-card/65 px-3 py-2 transition-[background-color,border-color,box-shadow,color] duration-200 hover:border-primary-strong/45"
                      key={asset.id}
                    >
                      <span className="flex items-center gap-3">
                        <input
                          checked={selected}
                          className="size-4 rounded border-theme-control-border accent-primary"
                          disabled={busy}
                          onChange={() => toggleManualAsset(asset.id)}
                          type="checkbox"
                        />
                        <AssetPickerText asset={asset} />
                      </span>
                      {isRule && (
                        <span className="rounded border border-primary-strong/35 bg-primary/10 px-2 py-0.5 text-label-sm font-semibold uppercase text-primary">
                          {t("group.origin.rule")}
                        </span>
                      )}
                    </label>
                  );
                })
              )}
            </div>
          </section>
        </form>
      </DialogFrame>

      {svgEditorOpen && (
        <DialogFrame
          closeLabel={t("group.icon.closeSvg")}
          footer={
            <>
              <Button onClick={clearIconSvg} type="button" variant="ghost">
                {t("group.icon.clearSvg")}
              </Button>
              <div className="flex items-center gap-2">
                <Button
                  onClick={closeSvgEditor}
                  type="button"
                  variant="outline"
                >
                  {t("group.icon.cancelSvg")}
                </Button>
                <Button onClick={saveIconSvg} type="button">
                  {t("group.icon.saveSvg")}
                </Button>
              </div>
            </>
          }
          footerClassName="justify-between"
          icon={<Code2 size={18} />}
          iconClassName="border-primary-strong/25 bg-primary/15 text-primary"
          onClose={closeSvgEditor}
          containerClassName="px-6"
          layer="nested"
          size="xl"
          title={t("group.icon.svgEditorTitle")}
        >
          <div className="flex min-h-0 flex-col gap-3">
            <p className="text-body-sm text-on-surface-variant">
              {t("group.icon.svgEditorDescription")}
            </p>
            <label className="flex min-h-0 flex-1 flex-col gap-2">
              <span className="text-label-caps uppercase text-outline">
                {t("group.icon.svgInput")}
              </span>
              <textarea
                aria-label={t("group.icon.svgInput")}
                className="min-h-80 resize-y rounded-xl border border-theme-control-border bg-theme-control px-3 py-3 font-mono text-code-md text-on-surface outline-none transition-[background-color,border-color,box-shadow,color] duration-200 placeholder:text-outline focus:border-primary-strong/60"
                onChange={(event) => setSvgDraft(event.target.value)}
                placeholder={t("group.icon.svgPlaceholder")}
                spellCheck={false}
                value={svgDraft}
              />
            </label>
            {svgError && (
              <p className="text-body-sm text-status-remove">{svgError}</p>
            )}
          </div>
        </DialogFrame>
      )}
    </>
  );
}

function filterAssets(assets: Asset[], query: string) {
  const normalizedQuery = query.trim().toLowerCase();
  if (!normalizedQuery) {
    return assets;
  }

  return assets.filter((asset) =>
    [asset.name, asset.description ?? "", asset.relative_path]
      .join(" ")
      .toLowerCase()
      .includes(normalizedQuery),
  );
}
