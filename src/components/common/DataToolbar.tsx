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
  leading,
  sticky = false,
}: {
  actions: React.ReactNode;
  ariaLabel: string;
  leading: React.ReactNode;
  sticky?: boolean;
}) {
  return (
    <section
      aria-label={ariaLabel}
      className={clsx(
        "flex items-center justify-between gap-3 max-[1160px]:flex-col max-[1160px]:items-stretch",
        sticky &&
          "sticky top-[var(--app-toolbar-top)] z-10 border-b border-border bg-surface-low/75 px-[var(--app-page-x)] py-[var(--app-toolbar-y)] shadow-[0_12px_28px_rgba(2,8,23,0.22)] backdrop-blur",
      )}
    >
      <div className="flex min-w-0 flex-1 items-center gap-3 max-[1160px]:flex-wrap">{leading}</div>
      <div className="flex shrink-0 items-center justify-end gap-2 max-[1160px]:justify-start max-[1160px]:flex-wrap">
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
        "flex h-10 min-w-72 items-center gap-2 rounded-xl border border-border bg-surface-high/90 px-3 text-outline shadow-[inset_0_1px_0_rgba(255,255,255,0.04)] transition-colors focus-within:border-primary/60 focus-within:text-primary",
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
    <div aria-label={ariaLabel} className="flex h-10 items-center rounded-xl border border-border bg-surface-high/90 p-1" role="group">
      {options.map((option) => (
        <button
          aria-label={option.label}
          aria-pressed={value === option.value}
          className={clsx(
            "grid size-8 place-items-center rounded-lg text-on-surface-variant transition-colors hover:text-on-surface",
            value === option.value ? "bg-surface-highest text-primary" : "hover:bg-surface-highest/60",
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
        text ? "min-w-[5.75rem] px-4 text-body-sm font-semibold" : "w-10",
        primary
          ? "bg-gradient-to-br from-status-update to-status-create/80 text-white shadow-glow hover:-translate-y-0.5"
          : "border border-border bg-surface-high/90 text-on-surface-variant shadow-[inset_0_1px_0_rgba(255,255,255,0.04)] hover:bg-surface-highest hover:text-on-surface",
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
      className="inline-flex h-10 items-center justify-center gap-2 whitespace-nowrap rounded-xl border border-border bg-surface-high/90 px-3 text-body-sm text-on-surface-variant shadow-[inset_0_1px_0_rgba(255,255,255,0.04)] transition-colors hover:bg-surface-highest hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-55"
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
    <div className="inline-flex h-10 min-w-24 items-center justify-between gap-3 rounded-xl border border-border bg-surface-high/70 px-3 text-body-sm shadow-[inset_0_1px_0_rgba(255,255,255,0.04)]">
      <span className="whitespace-nowrap text-on-surface-variant">{label}</span>
      <strong className="font-mono text-code-md text-primary">{value}</strong>
    </div>
  );
}

export function ToolbarSeparator() {
  return <span className="mx-1 h-6 w-px bg-border" aria-hidden="true" />;
}
