import { Code2, FolderPlus } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Switch } from "../ui/switch";
import { DialogFrame } from "../foundation/DialogFrame";
import { useI18n } from "../../i18n/I18nProvider";
import { DEFAULT_GROUP_COLOR_HEX } from "../../theme/themes";
import { isHexColor } from "../../theme/colorValidation";
import type { Asset, AssetGroupIconSvg, AssetGroupInput } from "../../types";
import { AssetPickerHeader, AssetPickerText, GroupField } from "./SkillGroupFormPrimitives";

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
  const nameInputRef = useRef<HTMLInputElement>(null);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [color, setColor] = useState(DEFAULT_GROUP_COLOR_HEX);
  const [draftColor, setDraftColor] = useState(DEFAULT_GROUP_COLOR_HEX);
  const [displayIcon, setDisplayIcon] = useState("");
  const [iconSvg, setIconSvg] = useState<AssetGroupIconSvg | null>(null);
  const [enabled, setEnabled] = useState(true);
  const [query, setQuery] = useState("");
  const [selectedAssetIds, setSelectedAssetIds] = useState<Set<string>>(new Set());
  const [formError, setFormError] = useState<string | null>(null);
  const [svgEditorOpen, setSvgEditorOpen] = useState(false);
  const [svgDraft, setSvgDraft] = useState("");
  const [svgError, setSvgError] = useState("");

  const skillAssets = useMemo(() => assets.filter((asset) => asset.kind === "skill"), [assets]);
  const filteredAssets = useMemo(() => filterAssets(skillAssets, query), [query, skillAssets]);
  const selectedCount = selectedAssetIds.size;

  useEffect(() => {
    if (!open) {
      return;
    }

    const randomColor = generateRandomGroupColor();
    setName("");
    setDescription("");
    setColor(randomColor);
    setDraftColor(randomColor);
    setDisplayIcon("");
    setIconSvg(null);
    setEnabled(true);
    setQuery("");
    setSelectedAssetIds(new Set());
    setFormError(null);
    setSvgEditorOpen(false);
    setSvgDraft("");
    setSvgError("");
  }, [open]);

  if (!open) {
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
        display_icon: displayIcon.trim() || null,
        icon_svg: iconSvg,
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

            <div className="grid grid-cols-[minmax(0,1fr)_auto_auto] items-end gap-3 max-[720px]:grid-cols-1">
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
                <div className="flex h-10 items-center gap-2">
                  <span
                    aria-hidden="true"
                    className="grid size-9 shrink-0 place-items-center rounded-lg border text-[13px] font-bold"
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
                    aria-label={t("group.field.iconText")}
                    className="h-9 w-20 border-theme-control-border bg-theme-control font-mono text-code-md"
                    disabled={busy}
                    maxLength={4}
                    onChange={(event) => setDisplayIcon(event.target.value)}
                    placeholder={t("group.field.iconHint")}
                    value={displayIcon}
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

              <label className="flex h-10 items-center gap-2 self-end rounded-lg border border-theme-control-border bg-theme-control px-3">
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
