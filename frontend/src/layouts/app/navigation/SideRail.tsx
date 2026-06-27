import clsx from "clsx";
import { PanelLeftClose, PanelLeftOpen } from "lucide-react";
import type { ReactNode } from "react";
import { useI18n } from "../../../i18n/I18nProvider";
import { headerTabLabel, railLabel } from "../../../i18n/navigation";
import { MenuIcon } from "../../../router/icons";
import type { HeaderTabItem, NavigationIcon, RailMenuItem } from "../../../router/types";

export interface SideRailBrandAction {
  ariaLabel: string;
  busy?: boolean;
  icon?: ReactNode;
  label: string;
  onClick: () => void;
  title?: string;
  tone?: "error" | "neutral" | "ready" | "update";
}

export function SideRail({
  activeId,
  activeHeaderTabId,
  brandAction,
  expanded,
  headerTabs,
  items,
  onExpandedChange,
  onHeaderTabSelect,
  onItemSelect,
  primaryAction,
}: {
  activeId: string;
  activeHeaderTabId: string;
  brandAction?: SideRailBrandAction;
  expanded: boolean;
  headerTabs: HeaderTabItem[];
  items: RailMenuItem[];
  onExpandedChange: (expanded: boolean) => void;
  onHeaderTabSelect: (tab: HeaderTabItem) => void;
  onItemSelect?: (item: RailMenuItem) => void;
  primaryAction?: ReactNode;
}) {
  const { t } = useI18n();
  const secondaryItems = items.filter((item) => item.enabled && item.position === "secondary");
  const enabledHeaderTabs = headerTabs.filter((tab) => tab.enabled);
  const toggleLabel = expanded ? t("nav.sidebar.collapse") : t("nav.sidebar.expand");
  const ToggleIcon = expanded ? PanelLeftClose : PanelLeftOpen;

  return (
    <aside
      className={clsx(
        "fixed inset-y-0 left-0 z-30 flex w-[var(--app-sidebar-width)] flex-col justify-between border-r border-theme-nav-active-border bg-theme-nav/95 px-2 py-4 backdrop-blur transition-[width] duration-200",
        expanded ? "items-stretch" : "items-center",
      )}
      aria-label={t("nav.aria.main")}
      data-expanded={expanded}
    >
      <div className={clsx("flex w-full flex-col gap-2", expanded ? "items-stretch" : "items-center")}>
        {primaryAction ? <div className="mb-2 flex w-full justify-center">{primaryAction}</div> : null}
        <div className={clsx("mb-4 flex w-full items-center gap-2", expanded ? "justify-between" : "flex-col")}>
          <BrandIdentity action={brandAction} expanded={expanded} />
          <button
            className="grid size-10 shrink-0 place-items-center rounded-xl border border-transparent text-on-surface-variant/75 transition-all hover:border-theme-nav-active-border hover:bg-theme-nav-hover hover:text-theme-nav-active-fg active:scale-95"
            aria-expanded={expanded}
            aria-label={toggleLabel}
            onClick={() => onExpandedChange(!expanded)}
            title={toggleLabel}
            type="button"
          >
            <ToggleIcon size={18} />
          </button>
        </div>
        <HeaderTabRailGroup activeId={activeHeaderTabId} expanded={expanded} tabs={enabledHeaderTabs} onSelect={onHeaderTabSelect} />
      </div>

      <div className={clsx("flex w-full flex-col gap-2", expanded ? "items-stretch" : "items-center")}>
        <RailGroup activeId={activeId} expanded={expanded} items={secondaryItems} onItemSelect={onItemSelect} />
      </div>
    </aside>
  );
}

function BrandIdentity({
  action,
  expanded,
}: {
  action?: SideRailBrandAction;
  expanded: boolean;
}) {
  const toneClassName = getBrandActionToneClassName(action?.tone);
  const brandClassName = clsx(
    "flex h-10 items-center rounded-xl border border-theme-nav-active-border bg-theme-nav-active text-theme-nav-active-fg shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.26)]",
    expanded ? "min-w-0 flex-1 gap-3 px-3" : "size-10 justify-center",
    action && !expanded && toneClassName,
  );

  if (!expanded && action) {
    return (
      <button
        aria-label={action.ariaLabel}
        className={clsx(brandClassName, "relative transition-colors hover:bg-theme-nav-hover active:scale-95")}
        onClick={action.onClick}
        title={action.title ?? action.ariaLabel}
        type="button"
      >
        <MenuIcon name="rocket" size={22} />
        {action.icon ? (
          <span
            className={clsx(
              "absolute -right-1 -top-1 grid size-4 place-items-center rounded-full border border-theme-nav bg-theme-card shadow-[0_4px_10px_rgb(var(--theme-panel-shadow)/0.2)]",
              getBrandActionIconToneClassName(action.tone),
              action.busy && "animate-spin",
            )}
          >
            {action.icon}
          </span>
        ) : null}
      </button>
    );
  }

  return (
    <div className={brandClassName} title="AssetIWeave">
      <MenuIcon name="rocket" size={22} />
      {expanded ? (
        action ? (
          <button
            aria-label={action.ariaLabel}
            className={clsx(
              "flex min-w-0 flex-1 items-center gap-1.5 rounded-lg px-1.5 py-1 text-left text-body-sm font-semibold transition-colors hover:bg-theme-nav-hover active:scale-[0.98]",
              toneClassName,
            )}
            onClick={action.onClick}
            title={action.title ?? action.ariaLabel}
            type="button"
          >
            {action.icon ? (
              <span className={clsx("grid size-4 shrink-0 place-items-center", action.busy && "animate-spin")}>
                {action.icon}
              </span>
            ) : null}
            <span className="min-w-0 truncate" data-side-rail-label="">
              {action.label}
            </span>
          </button>
        ) : (
          <span className="min-w-0 truncate text-body-md font-semibold" data-side-rail-label="">
            AssetIWeave
          </span>
        )
      ) : null}
    </div>
  );
}

function getBrandActionToneClassName(tone: SideRailBrandAction["tone"] | undefined) {
  if (!tone || tone === "neutral") {
    return "";
  }
  if (tone === "error") {
    return "border-status-remove/45 bg-status-remove/15 text-status-remove hover:text-status-remove";
  }
  if (tone === "ready") {
    return "border-status-create/45 bg-status-create/15 text-status-create hover:text-status-create";
  }
  return "border-status-update/45 bg-status-update/15 text-status-update hover:text-status-update";
}

function getBrandActionIconToneClassName(tone: SideRailBrandAction["tone"] | undefined) {
  if (!tone || tone === "neutral") {
    return "text-theme-nav-active-fg";
  }
  if (tone === "error") {
    return "text-status-remove";
  }
  if (tone === "ready") {
    return "text-status-create";
  }
  return "text-status-update";
}

function HeaderTabRailGroup({
  activeId,
  expanded,
  onSelect,
  tabs,
}: {
  activeId: string;
  expanded: boolean;
  onSelect: (tab: HeaderTabItem) => void;
  tabs: HeaderTabItem[];
}) {
  const { locale, t } = useI18n();

  return (
    <nav className={clsx("flex w-full flex-col gap-2", expanded ? "items-stretch" : "items-center")} aria-label={t("nav.aria.assetTypes")}>
      {tabs.map((tab) => {
        const label = headerTabLabel(tab, t, locale);
        const selected = tab.id === activeId;

        return (
          <RailButton
            active={selected}
            expanded={expanded}
            icon={headerTabIcon(tab)}
            key={tab.id}
            label={label}
            onClick={() => onSelect(tab)}
          />
        );
      })}
    </nav>
  );
}

function RailGroup({
  activeId,
  expanded,
  items,
  onItemSelect,
}: {
  activeId: string;
  expanded: boolean;
  items: RailMenuItem[];
  onItemSelect?: (item: RailMenuItem) => void;
}) {
  const { locale, t } = useI18n();

  return (
    <nav className={clsx("flex w-full flex-col gap-2", expanded ? "items-stretch" : "items-center")}>
      {items.map((item) => {
        const label = railLabel(item, t, locale);

        return (
          <RailButton
            active={item.id === activeId}
            expanded={expanded}
            icon={item.icon}
            key={item.id}
            label={label}
            onClick={() => onItemSelect?.(item)}
          />
        );
      })}
    </nav>
  );
}

function RailButton({
  active,
  expanded,
  icon,
  label,
  onClick,
}: {
  active: boolean;
  expanded: boolean;
  icon: NavigationIcon;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      className={clsx(
        "flex h-10 min-w-0 items-center rounded-xl border transition-all active:scale-95",
        expanded ? "w-full justify-start gap-3 px-3" : "size-10 justify-center",
        active
          ? "border-theme-nav-active-border bg-theme-nav-active text-theme-nav-active-fg"
          : "border-transparent text-on-surface-variant/75 hover:border-theme-nav-active-border hover:bg-theme-nav-hover hover:text-theme-nav-active-fg",
      )}
      aria-label={label}
      aria-current={active ? "page" : undefined}
      onClick={onClick}
      title={label}
      type="button"
    >
      <span className="grid size-5 shrink-0 place-items-center">
        <MenuIcon name={icon} />
      </span>
      {expanded ? (
        <span className="min-w-0 truncate text-left text-body-sm font-medium" data-side-rail-label="">
          {label}
        </span>
      ) : null}
    </button>
  );
}

function headerTabIcon(tab: HeaderTabItem): NavigationIcon {
  if (tab.id === "conversations") return "file-text";

  switch (tab.assetKind) {
    case "skill":
      return "sparkles";
    case "mcp":
      return "grid";
    case "prompt":
      return "file-code";
    case "rule":
      return "shield";
    case "profile":
      return "boxes";
    default:
      return "archive";
  }
}
