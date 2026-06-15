import { Code2, Save } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Switch } from "../ui/switch";
import { DialogFrame } from "../foundation/DialogFrame";
import { useI18n } from "../../i18n/I18nProvider";
import { DEFAULT_GROUP_COLOR_HEX } from "../../theme/themes";
import { isHexColor } from "../../theme/colorValidation";
import type { Asset, AssetGroup, AssetGroupDetail, AssetGroupIconSvg } from "../../types";
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
  const [draftColor, setDraftColor] = useState(detail?.group.color ?? DEFAULT_GROUP_COLOR_HEX);
  const [displayIcon, setDisplayIcon] = useState(detail?.group.display_icon ?? "");
  const [iconSvg, setIconSvg] = useState<AssetGroupIconSvg | null>(detail?.group.icon_svg ?? null);
  const [enabled, setEnabled] = useState(detail?.group.enabled ?? true);
  const [query, setQuery] = useState("");
  const [manualAssetIds, setManualAssetIds] = useState<Set<string>>(() => new Set(detail?.manual_asset_ids ?? []));
  const [formError, setFormError] = useState<string | null>(null);
  const [svgEditorOpen, setSvgEditorOpen] = useState(false);
  const [svgDraft, setSvgDraft] = useState("");
  const [svgError, setSvgError] = useState("");

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
    const nextColor = detail?.group.color ?? DEFAULT_GROUP_COLOR_HEX;
    setColor(nextColor);
    setDraftColor(nextColor);
    setDisplayIcon(detail?.group.display_icon ?? "");
    setIconSvg(detail?.group.icon_svg ?? null);
    setEnabled(detail?.group.enabled ?? true);
    setManualAssetIds(new Set(detail?.manual_asset_ids ?? []));
    setQuery("");
    setFormError(null);
    setSvgEditorOpen(false);
    setSvgDraft("");
    setSvgError("");
  }, [detail]);

  if (!detail) {
    return null;
  }

  function commitColor(nextColor: string) {
    const trimmed = nextColor.trim();
    if (!isHexColor(trimmed)) {
      setDraftColor(color);
      return;
    }
    const normalized = trimmed.toLowerCase();
    setColor(normalized);
    setDraftColor(normalized);
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
      setIconSvg(null);
      closeSvgEditor();
      return;
    }

    const result = parseGroupIconSvgInput(input);
    if (result) {
      setIconSvg(result);
      closeSvgEditor();
      return;
    }

    setSvgError(t("group.icon.svgError"));
  }

  function clearIconSvg() {
    setIconSvg(null);
    closeSvgEditor();
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
        display_icon: displayIcon.trim() || null,
        icon_svg: iconSvg,
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
    <>
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
          <section className="grid gap-4 rounded-xl border border-theme-card-border bg-theme-card/65 p-4">
            <div className="grid grid-cols-[17rem_minmax(0,1fr)] gap-4 max-[760px]:grid-cols-1">
              <div className="grid gap-3 rounded-lg border border-theme-card-border bg-theme-card-header/45 p-4">
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
                    <p className="truncate text-title-sm font-bold text-on-surface">{name.trim() || detail.group.name}</p>
                    <p className="mt-1 truncate text-body-sm text-on-surface-variant">
                      {description.trim() || detail.group.description || t("group.noDescription")}
                    </p>
                  </div>
                </div>
                <label className="flex h-10 w-full items-center justify-between gap-3 rounded-lg border border-theme-control-border bg-theme-control px-3">
                  <Switch checked={enabled} disabled={busy} onCheckedChange={setEnabled} />
                  <span className="whitespace-nowrap text-body-sm text-on-surface-variant">{t("group.field.enabled")}</span>
                </label>
              </div>

              <div className="grid min-w-0 gap-4">
                <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-3 max-[760px]:grid-cols-1">
                  <GroupField label={t("group.field.name")}>
                    <Input disabled={busy} onChange={(event) => setName(event.target.value)} ref={nameInputRef} value={name} />
                  </GroupField>
                  <GroupField label={t("group.field.description")}>
                    <Input disabled={busy} onChange={(event) => setDescription(event.target.value)} value={description} />
                  </GroupField>
                </div>

                <div className="grid grid-cols-[minmax(0,1fr)_minmax(18rem,1.1fr)] items-end gap-3 max-[900px]:grid-cols-1">
                  <GroupField label={t("group.field.colorCode")}>
                    <div className="flex h-10 items-center gap-2 rounded-lg border border-theme-control-border bg-theme-control px-2 transition-colors focus-within:border-primary-strong/60">
                      <input
                        aria-label={t("group.field.color")}
                        className="size-5 shrink-0 cursor-pointer rounded border-0 bg-transparent p-0"
                        disabled={busy}
                        onChange={(event) => {
                          const nextColor = event.target.value;
                          setColor(nextColor);
                          setDraftColor(nextColor);
                        }}
                        type="color"
                        value={color}
                      />
                      <Input
                        aria-label={t("group.field.colorCode")}
                        className="h-auto min-w-0 flex-1 border-0 bg-transparent p-0 font-mono text-code-md focus:border-transparent"
                        disabled={busy}
                        maxLength={7}
                        onBlur={(event) => commitColor(event.currentTarget.value)}
                        onChange={(event) => setDraftColor(event.target.value.slice(0, 7))}
                        onKeyDown={(event) => {
                          if (event.key === "Enter") {
                            commitColor(event.currentTarget.value);
                            event.currentTarget.blur();
                          }
                          if (event.key === "Escape") {
                            setDraftColor(color);
                            event.currentTarget.blur();
                          }
                        }}
                        value={draftColor}
                      />
                    </div>
                  </GroupField>

                  <GroupField label={t("group.field.icon")}>
                    <div className="flex h-10 min-w-0 items-center gap-2">
                      <Input
                        aria-label={t("group.field.iconText")}
                        className="h-10 min-w-0 flex-1 border-theme-control-border bg-theme-control font-mono text-code-md"
                        disabled={busy}
                        maxLength={4}
                        onChange={(event) => setDisplayIcon(event.target.value)}
                        placeholder={t("group.field.iconHint")}
                        value={displayIcon}
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

      {svgEditorOpen && (
        <DialogFrame
          closeLabel={t("group.icon.closeSvg")}
          footer={
            <>
              <Button onClick={clearIconSvg} type="button" variant="ghost">
                {t("group.icon.clearSvg")}
              </Button>
              <div className="flex items-center gap-2">
                <Button onClick={closeSvgEditor} type="button" variant="outline">
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
          overlayClassName="z-[60] px-6"
          size="xl"
          title={t("group.icon.svgEditorTitle")}
        >
          <div className="flex min-h-0 flex-col gap-3">
            <p className="text-body-sm text-on-surface-variant">{t("group.icon.svgEditorDescription")}</p>
            <label className="flex min-h-0 flex-1 flex-col gap-2">
              <span className="text-label-caps uppercase text-outline">{t("group.icon.svgInput")}</span>
              <textarea
                aria-label={t("group.icon.svgInput")}
                className="min-h-80 resize-y rounded-lg border border-theme-control-border bg-theme-control px-3 py-3 font-mono text-code-md text-on-surface outline-none transition-colors placeholder:text-outline focus:border-primary-strong/60"
                onChange={(event) => setSvgDraft(event.target.value)}
                placeholder={t("group.icon.svgPlaceholder")}
                spellCheck={false}
                value={svgDraft}
              />
            </label>
            {svgError && <p className="text-body-sm text-status-remove">{svgError}</p>}
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
    [asset.name, asset.description ?? "", asset.relative_path].join(" ").toLowerCase().includes(normalizedQuery),
  );
}

function parseGroupIconSvgInput(value: string): AssetGroupIconSvg | null {
  const input = value.trim();
  if (!input) {
    return null;
  }

  // Try JSON parse first
  try {
    const candidate = JSON.parse(input);
    if (isRecord(candidate) && Array.isArray(candidate.paths)) {
      const paths = candidate.paths.flatMap((path: unknown) => {
        if (!isRecord(path) || typeof path.d !== "string") return [];
        const d = path.d.trim();
        if (!d) return [];
        const clipRule = normalizeSvgRule(path.clip_rule ?? path.clipRule);
        const fillRule = normalizeSvgRule(path.fill_rule ?? path.fillRule);
        return [{ d, ...(clipRule ? { clip_rule: clipRule } : {}), ...(fillRule ? { fill_rule: fillRule } : {}) }];
      });
      if (paths.length === 0) return null;
      return {
        paths,
        ...(typeof candidate.viewBox === "string" && candidate.viewBox.trim()
          ? { view_box: candidate.viewBox.trim() }
          : typeof candidate.view_box === "string" && candidate.view_box.trim()
            ? { view_box: candidate.view_box.trim() }
            : {}),
      };
    }
  } catch {
    // Fall through to SVG markup parsing
  }

  // Try parsing as SVG markup
  if (input.includes("<svg") && typeof DOMParser !== "undefined") {
    try {
      const document = new DOMParser().parseFromString(input, "image/svg+xml");
      if (document.querySelector("parsererror")) return null;
      const svg = document.querySelector("svg");
      if (!svg) return null;

      const paths = Array.from(svg.querySelectorAll("path")).flatMap((path) => {
        const d = path.getAttribute("d")?.trim();
        if (!d) return [];
        const clipRule = normalizeSvgRule(
          path.getAttribute("clip-rule") ?? path.getAttribute("clipRule"),
        );
        const fillRule = normalizeSvgRule(
          path.getAttribute("fill-rule") ?? path.getAttribute("fillRule"),
        );
        return [{ d, ...(clipRule ? { clip_rule: clipRule } : {}), ...(fillRule ? { fill_rule: fillRule } : {}) }];
      });
      if (paths.length === 0) return null;

      const viewBox = svg.getAttribute("viewBox")?.trim();
      return {
        paths,
        ...(viewBox ? { view_box: viewBox } : {}),
      };
    } catch {
      return null;
    }
  }

  return null;
}

function normalizeSvgRule(value: unknown): "evenodd" | "nonzero" | null {
  return value === "evenodd" || value === "nonzero" ? value : null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === "object" && !Array.isArray(value));
}
