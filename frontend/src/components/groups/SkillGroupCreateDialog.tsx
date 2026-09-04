import { zodResolver } from "@hookform/resolvers/zod";
import { Code2, FolderPlus } from "lucide-react";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { Controller, useForm } from "react-hook-form";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Switch } from "../ui/switch";
import { DialogFrame } from "../foundation/DialogFrame";
import { useI18n } from "../../i18n/I18nProvider";
import { DEFAULT_GROUP_COLOR_HEX } from "../../theme/themes";
import { isHexColor } from "../../theme/colorValidation";
import type { Asset, AssetGroupInput } from "../../types";
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

function generateRandomGroupColor(): string {
  const hue = Math.floor(Math.random() * 360);
  const saturation = 50 + Math.floor(Math.random() * 30); // 50-80%
  const lightness = 40 + Math.floor(Math.random() * 20); // 40-60%
  // Convert HSL to hex
  const s = saturation / 100;
  const l = lightness / 100;
  const a = s * Math.min(l, 1 - l);
  const f = (n: number) => {
    const k = (n + hue / 30) % 12;
    const color = l - a * Math.max(Math.min(k - 3, 9 - k, 1), -1);
    return Math.round(255 * color)
      .toString(16)
      .padStart(2, "0");
  };
  return `#${f(0)}${f(8)}${f(4)}`;
}

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
      name: "",
      description: "",
      color: DEFAULT_GROUP_COLOR_HEX,
      displayIcon: "",
      iconSvg: null,
      enabled: true,
    },
  });

  const [query, setQuery] = useState("");
  const [selectedAssetIds, setSelectedAssetIds] = useState<Set<string>>(
    new Set(),
  );
  const [svgEditorOpen, setSvgEditorOpen] = useState(false);
  const [svgDraft, setSvgDraft] = useState("");
  const [svgError, setSvgError] = useState("");

  const color = watch("color");
  const displayIcon = watch("displayIcon");
  const iconSvg = watch("iconSvg");

  const skillAssets = useMemo(
    () => assets.filter((asset) => asset.kind === "skill"),
    [assets],
  );
  const filteredAssets = useMemo(
    () => filterAssets(skillAssets, query),
    [query, skillAssets],
  );
  const selectedCount = selectedAssetIds.size;

  useEffect(() => {
    if (!open) {
      return;
    }

    const randomColor = generateRandomGroupColor();
    reset({
      name: "",
      description: "",
      color: randomColor,
      displayIcon: "",
      iconSvg: null,
      enabled: true,
    });
    setQuery("");
    setSelectedAssetIds(new Set());
    setSvgEditorOpen(false);
    setSvgDraft("");
    setSvgError("");
  }, [open, reset]);

  if (!open) {
    return null;
  }

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
    const input = groupValuesToInput(values);
    await onSubmit(
      {
        ...input,
        sort_order: nextSortOrder,
        rules: { source_ids: [], relative_path_globs: [], name_contains: null },
      },
      [...selectedAssetIds],
    );
  };

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
      const allVisibleSelected =
        filteredAssets.length > 0 &&
        filteredAssets.every((asset) => next.has(asset.id));
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
      <Button disabled={busy} form={formId} type="submit">
        {t("group.createDialog.submit")}
      </Button>
    </div>
  );

  const { ref: nameFormRef, ...nameRegisterRest } = register("name");

  return (
    <>
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
        <form
          className="grid gap-4"
          id={formId}
          onSubmit={handleSubmit(onSubmitValid)}
        >
          <section className="grid gap-2 rounded-xl border border-theme-card-border bg-theme-card/65 p-3">
            <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] items-end gap-2 max-[720px]:grid-cols-1">
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
                <Input
                  {...register("description")}
                  disabled={busy}
                />
              </GroupField>
            </div>

            <div className="grid grid-cols-[minmax(0,1fr)_auto_auto] items-end gap-3 max-[720px]:grid-cols-1">
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
                        onChange={(event) => field.onChange(event.target.value)}
                        value={field.value}
                      />
                    </div>
                  </GroupField>
                )}
              />

              <GroupField label={t("group.field.icon")}>
                <div className="flex h-10 items-center gap-2">
                  <span
                    aria-hidden="true"
                    className="grid size-9 shrink-0 place-items-center rounded-xl border text-[13px] font-bold"
                    style={{
                      borderColor: `${color}66`,
                      backgroundColor: `${color}18`,
                      color,
                    }}
                  >
                    {iconSvg && iconSvg.paths.length > 0 ? (
                      <svg
                        aria-hidden="true"
                        className="size-4"
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
                      <FolderPlus size={16} />
                    )}
                  </span>
                  <Input
                    {...register("displayIcon")}
                    aria-label={t("group.field.iconText")}
                    className="h-9 w-20 border-theme-control-border bg-theme-control font-mono text-code-md"
                    disabled={busy}
                    maxLength={4}
                    placeholder={t("group.field.iconHint")}
                  />
                  <Button
                    aria-label={t("group.icon.editSvg")}
                    className="h-9 shrink-0 px-2.5"
                    disabled={busy}
                    onClick={openSvgEditor}
                    title={t("group.icon.editSvg")}
                    type="button"
                    variant="outline"
                  >
                    <Code2 size={15} />
                  </Button>
                </div>
              </GroupField>

              <Controller
                control={control}
                name="enabled"
                render={({ field }) => (
                  <label className="flex h-10 items-center gap-2 self-end rounded-xl border border-theme-control-border bg-theme-control px-3">
                    <Switch
                      checked={field.value}
                      disabled={busy}
                      onCheckedChange={field.onChange}
                    />
                    <span className="text-body-sm text-on-surface-variant">
                      {t("group.field.enabled")}
                    </span>
                  </label>
                )}
              />
            </div>
          </section>

          <section className="flex min-h-0 flex-1 flex-col gap-3">
            <AssetPickerHeader
              onQueryChange={setQuery}
              onToggleAll={toggleAllVisible}
              query={query}
              selectedCount={selectedCount}
              title={t("group.createDialog.assets")}
              totalCount={skillAssets.length}
            />

            <div className="grid max-h-72 gap-2 overflow-y-auto">
              {filteredAssets.length === 0 ? (
                <div className="rounded-xl border border-theme-card-border bg-theme-card/35 px-4 py-8 text-center text-body-sm text-on-surface-variant">
                  {t("group.assets.empty")}
                </div>
              ) : (
                filteredAssets.map((asset) => {
                  const selected = selectedAssetIds.has(asset.id);
                  return (
                    <label
                      className="flex cursor-pointer items-center gap-3 rounded-xl border border-theme-card-border bg-theme-card/65 px-3 py-2 transition-[background-color,border-color,box-shadow,color] duration-200 hover:border-primary-strong/45"
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
