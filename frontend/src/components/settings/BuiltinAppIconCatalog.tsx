import { AppShortcutIcon, appIconToken } from "../apps/AppShortcutIcon";
import { appShortcutIconCatalog } from "../../config/appShortcutIcons";

export function BuiltinAppIconCatalog({ title }: { title: string }) {
  return (
    <section className="border-t border-theme-card-border px-4 py-4" aria-labelledby="builtin-app-icon-catalog-title">
      <h3 className="text-label-caps uppercase text-outline" id="builtin-app-icon-catalog-title">
        {title}
      </h3>
      <div aria-label={title} className="mt-3 grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-4" role="list">
        {appShortcutIconCatalog.map(({ appKind }) => (
          <div
            className="flex min-w-0 items-center gap-2 rounded-lg border border-theme-control-border/70 bg-theme-control/45 px-2.5 py-2"
            key={appKind}
            role="listitem"
            title={appKind}
          >
            <span className="grid size-7 shrink-0 place-items-center rounded-md border border-theme-control-border bg-theme-card text-primary">
              <AppShortcutIcon appKind={appKind} className="size-4" displayIcon={appIconToken(appKind)} />
            </span>
            <span className="truncate text-body-sm text-on-surface-variant">{appKind}</span>
          </div>
        ))}
      </div>
    </section>
  );
}
