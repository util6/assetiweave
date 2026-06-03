import clsx from "clsx";
import { useI18n } from "../../../i18n/I18nProvider";
import { railLabel } from "../../../i18n/navigation";
import { MenuIcon } from "../../../router/icons";
import type { RailMenuItem } from "../../../router/types";

export function SideRail({
  activeId,
  items,
  onItemSelect,
}: {
  activeId: string;
  items: RailMenuItem[];
  onItemSelect?: (item: RailMenuItem) => void;
}) {
  const { t } = useI18n();
  const primaryItems = items.filter((item) => item.enabled && item.position === "primary");
  const secondaryItems = items.filter((item) => item.enabled && item.position === "secondary");

  return (
    <aside
      className="fixed inset-y-0 left-0 z-30 flex w-sidebar-width flex-col items-center justify-between border-r border-theme-nav-active-border bg-theme-nav/95 px-2 py-4 backdrop-blur"
      aria-label={t("nav.aria.main")}
    >
      <div className="flex w-full flex-col items-center gap-2">
        <button
          className="mb-4 grid size-10 place-items-center rounded-xl border border-theme-nav-active-border bg-theme-nav-active text-theme-nav-active-fg transition-transform active:scale-95"
          aria-label="AssetIWeave"
        >
          <MenuIcon name="rocket" size={22} />
        </button>
        <RailGroup activeId={activeId} items={primaryItems} onItemSelect={onItemSelect} />
      </div>

      <RailGroup activeId={activeId} items={secondaryItems} onItemSelect={onItemSelect} />
    </aside>
  );
}

function RailGroup({
  activeId,
  items,
  onItemSelect,
}: {
  activeId: string;
  items: RailMenuItem[];
  onItemSelect?: (item: RailMenuItem) => void;
}) {
  const { locale, t } = useI18n();

  return (
    <nav className="flex w-full flex-col items-center gap-2">
      {items.map((item) => {
        const label = railLabel(item, t, locale);

        return (
          <button
            className={clsx(
              "grid size-10 place-items-center rounded-xl border transition-all active:scale-95",
              item.id === activeId
                ? "border-theme-nav-active-border bg-theme-nav-active text-theme-nav-active-fg"
                : "border-transparent text-on-surface-variant/75 hover:border-theme-nav-active-border hover:bg-theme-nav-hover hover:text-theme-nav-active-fg",
            )}
            key={item.id}
            aria-label={label}
            aria-current={item.id === activeId ? "page" : undefined}
            onClick={() => onItemSelect?.(item)}
            title={label}
            type="button"
          >
            <MenuIcon name={item.icon} />
          </button>
        );
      })}
    </nav>
  );
}
