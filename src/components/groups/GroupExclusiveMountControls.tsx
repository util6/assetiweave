import clsx from "clsx";
import { CheckCheck, Minus } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import type { AppShortcut, TargetProfile } from "../../types";
import { AppShortcutIconForShortcut } from "../apps/AppShortcutIcon";

export type GroupMountMode = "exclusive" | "additive";

export function GroupExclusiveMountControls({
  allSelected,
  appShortcuts,
  busy,
  mode,
  onModeChange,
  onPreviewProfile,
  onToggleAll,
  partiallySelected,
  profiles,
  selectableGroupCount,
  selectedGroupCount,
  selectedSkillCount,
}: {
  allSelected: boolean;
  appShortcuts: AppShortcut[];
  busy: boolean;
  mode: GroupMountMode;
  onModeChange: (mode: GroupMountMode) => void;
  onPreviewProfile: (shortcut: AppShortcut) => void | Promise<void>;
  onToggleAll: () => void;
  partiallySelected: boolean;
  profiles: TargetProfile[];
  selectableGroupCount: number;
  selectedGroupCount: number;
  selectedSkillCount: number;
}) {
  const { t } = useI18n();
  const availableShortcuts = appShortcuts
    .filter((shortcut) => shortcut.enabled)
    .filter((shortcut) =>
      profiles.some(
        (profile) =>
          profile.id === shortcut.profileId &&
          profile.enabled &&
          profile.supported_kinds.includes("skill"),
      ),
    );

  return (
    <section className="rounded-xl border border-primary/30 bg-primary/10 px-3 py-3 shadow-[inset_0_1px_0_rgba(255,255,255,0.04)]">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex min-w-0 flex-wrap items-center gap-3">
          <button
            aria-label={allSelected ? t("group.exclusive.clearAll") : t("group.exclusive.selectAll")}
            aria-pressed={allSelected}
            className={clsx(
              "grid size-9 place-items-center rounded-lg border text-primary transition-colors disabled:cursor-not-allowed disabled:opacity-45",
              allSelected || partiallySelected
                ? "border-primary/55 bg-primary/15"
                : "border-border bg-surface-high hover:border-primary/45 hover:bg-surface-highest",
            )}
            disabled={busy || selectableGroupCount === 0}
            onClick={onToggleAll}
            title={allSelected ? t("group.exclusive.clearAll") : t("group.exclusive.selectAll")}
            type="button"
          >
            {partiallySelected && !allSelected ? <Minus size={17} /> : <CheckCheck size={17} />}
          </button>
          <span className="text-body-sm font-semibold text-on-surface">
            {t("group.exclusive.selectedGroups", { count: selectedGroupCount })}
          </span>
          <span className="rounded-md border border-border bg-surface-high px-2 py-0.5 font-mono text-body-sm text-primary">
            {t("group.exclusive.selectedSkills", { count: selectedSkillCount })}
          </span>
          <div className="flex items-center gap-1 rounded-lg border border-border/80 bg-surface-high p-1">
            <ModeChoice
              checked={mode === "exclusive"}
              disabled={busy}
              label={t("group.exclusive.modeExclusive")}
              name="group-mount-mode"
              onChange={() => onModeChange("exclusive")}
            />
            <ModeChoice
              checked={mode === "additive"}
              disabled={busy}
              label={t("group.exclusive.modeAdditive")}
              name="group-mount-mode"
              onChange={() => onModeChange("additive")}
            />
          </div>
        </div>

        <div className="flex flex-wrap items-center justify-end gap-1.5">
          {availableShortcuts.length === 0 ? (
            <span className="text-body-sm text-on-surface-variant">{t("group.mount.noApps")}</span>
          ) : (
            availableShortcuts.map((shortcut) => (
              <button
                aria-label={targetActionLabel(mode, shortcut.profileName, t)}
                className={clsx(
                  "relative grid size-9 place-items-center overflow-hidden rounded-lg border text-[13px] font-bold transition-all hover:opacity-100 disabled:cursor-not-allowed disabled:opacity-40",
                )}
                disabled={busy || selectedSkillCount === 0}
                key={shortcut.profileId}
                onClick={() => void onPreviewProfile(shortcut)}
                style={{
                  borderColor: `${shortcut.accentColor}88`,
                  backgroundColor: `${shortcut.accentColor}16`,
                  color: shortcut.accentColor,
                }}
                title={targetActionLabel(mode, shortcut.profileName, t)}
                type="button"
              >
                <AppShortcutIconForShortcut className="size-4" shortcut={shortcut} />
              </button>
            ))
          )}
        </div>
      </div>
    </section>
  );
}

function ModeChoice({
  checked,
  disabled,
  label,
  name,
  onChange,
}: {
  checked: boolean;
  disabled: boolean;
  label: string;
  name: string;
  onChange: () => void;
}) {
  return (
    <label
      className={clsx(
        "inline-flex h-7 cursor-pointer items-center gap-1.5 rounded-md px-2 text-body-sm font-semibold transition-colors",
        checked ? "bg-primary/15 text-primary" : "text-on-surface-variant hover:bg-surface-highest hover:text-on-surface",
        disabled && "cursor-not-allowed opacity-45",
      )}
    >
      <input
        checked={checked}
        className="size-3.5 accent-primary"
        disabled={disabled}
        name={name}
        onChange={onChange}
        type="radio"
      />
      <span>{label}</span>
    </label>
  );
}

function targetActionLabel(
  mode: GroupMountMode,
  profileName: string,
  t: (key: "group.exclusive.keepOnlyTo" | "group.exclusive.addOnlyTo", params?: Record<string, string | number>) => string,
) {
  return t(mode === "exclusive" ? "group.exclusive.keepOnlyTo" : "group.exclusive.addOnlyTo", {
    profile: profileName,
  });
}
