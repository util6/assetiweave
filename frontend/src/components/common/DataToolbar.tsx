import clsx from "clsx";
import { Search } from "lucide-react";

export type ToolbarViewMode = "list" | "columns" | "grid";

export interface ToolbarViewOption<Value extends ToolbarViewMode = ToolbarViewMode> {
  icon: React.ReactNode;
  label: string;
  value: Value;
}

export function DataToolbar({
  actions,
  ariaLabel,
  compact = false,
  leading,
  sticky = false,
  stickyBleed = false,
}: {
  actions: React.ReactNode;
  ariaLabel: string;
  compact?: boolean;
  leading: React.ReactNode;
  sticky?: boolean;
  stickyBleed?: boolean;
}) {
  return (
    <section
      aria-label={ariaLabel}
      className={clsx(
        "grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 max-[820px]:grid-cols-1 max-[820px]:items-stretch",
        compact && "gap-2",
        sticky &&
          "sticky top-[calc(var(--app-toolbar-top)+var(--app-notification-offset,0px))] z-10 border-b border-theme-card-border bg-theme-toolbar/85 px-[var(--app-page-x)] py-[var(--app-toolbar-y)] shadow-[0_12px_28px_rgb(var(--theme-panel-shadow)/0.18)] backdrop-blur",
        sticky && stickyBleed && "-mx-[var(--app-page-x)]",
      )}
    >
      <div className={clsx("flex min-w-0 flex-wrap items-center gap-3", compact && "gap-2")}>{leading}</div>
      <div className="flex min-w-0 flex-wrap items-center justify-end gap-2 max-[820px]:justify-start">
        {actions}
      </div>
    </section>
  );
}

export function ToolbarSearch({
  className,
  onChange,
  placeholder,
  value,
}: {
  className?: string;
  onChange: (value: string) => void;
  placeholder: string;
  value: string;
}) {
  return (
    <label
      className={clsx(
        "flex h-10 min-w-[16rem] items-center gap-2 rounded-xl border border-theme-control-border bg-theme-control/95 px-3 text-outline shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.42)] transition-colors focus-within:border-primary/60 focus-within:text-primary",
        className,
      )}
    >
      <Search size={17} />
      <input
        className="min-w-0 flex-1 border-0 bg-transparent text-body-sm text-on-surface outline-none placeholder:text-outline"
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
        value={value}
      />
    </label>
  );
}

export function ToolbarViewToggle<Value extends ToolbarViewMode>({
  ariaLabel,
  onChange,
  options,
  value,
}: {
  ariaLabel: string;
  onChange: (value: Value) => void;
  options: ToolbarViewOption<Value>[];
  value: Value;
}) {
  return (
    <div aria-label={ariaLabel} className="flex h-10 items-center rounded-xl border border-theme-control-border bg-theme-control/95 p-1" role="group">
      {options.map((option) => (
        <button
          aria-label={option.label}
          aria-pressed={value === option.value}
          className={clsx(
            "grid size-8 place-items-center rounded-lg text-on-surface-variant transition-colors hover:text-on-surface",
            value === option.value ? "bg-theme-control-hover text-primary" : "hover:bg-theme-control-hover/70",
          )}
          key={option.value}
          onClick={() => onChange(option.value)}
          title={option.label}
          type="button"
        >
          {option.icon}
        </button>
      ))}
    </div>
  );
}

export function ToolbarActionButton({
  disabled = false,
  icon,
  label,
  onClick,
  primary = false,
  text,
}: {
  disabled?: boolean;
  icon: React.ReactNode;
  label: string;
  onClick?: () => void;
  primary?: boolean;
  text?: string;
}) {
  return (
    <button
      aria-label={label}
      className={clsx(
        "inline-flex h-10 items-center justify-center gap-2 rounded-xl transition-all active:scale-95 disabled:cursor-not-allowed disabled:opacity-55",
        text ? "min-w-[5.75rem] px-3 text-body-sm font-semibold" : "w-10",
        primary
          ? "bg-theme-button-primary text-theme-button-primary-fg shadow-glow hover:-translate-y-0.5 hover:bg-theme-button-primary-hover"
          : "border border-theme-control-border bg-theme-control/95 text-theme-control-fg shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.42)] hover:bg-theme-control-hover hover:text-on-surface",
      )}
      disabled={disabled}
      onClick={onClick}
      title={label}
      type="button"
    >
      {icon}
      {text && <span>{text}</span>}
    </button>
  );
}

export function ToolbarTextButton({
  disabled = false,
  icon,
  label,
  onClick,
}: {
  disabled?: boolean;
  icon: React.ReactNode;
  label: string;
  onClick?: () => void;
}) {
  return (
    <button
      className="inline-flex h-10 items-center justify-center gap-2 whitespace-nowrap rounded-xl border border-theme-control-border bg-theme-control/95 px-3 text-body-sm text-theme-control-fg shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.42)] transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-55"
      disabled={disabled}
      onClick={onClick}
      type="button"
    >
      {icon}
      <span>{label}</span>
    </button>
  );
}

export function ToolbarMetric({ label, value }: { label: string; value: number }) {
  return (
    <div className="inline-flex h-10 min-w-[5.75rem] items-center justify-between gap-2 rounded-xl border border-theme-control-border bg-theme-control/80 px-3 text-body-sm shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.42)]">
      <span className="whitespace-nowrap text-on-surface-variant">{label}</span>
      <strong className="font-mono text-code-md text-primary">{value}</strong>
    </div>
  );
}

export function ToolbarSeparator() {
  return <span className="mx-1 h-6 w-px bg-theme-control-border" aria-hidden="true" />;
}
