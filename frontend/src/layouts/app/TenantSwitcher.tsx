import clsx from "clsx";
import { Building2, Loader2 } from "lucide-react";
import { useId, useRef, useState, type FormEvent, type ReactNode } from "react";
import { DialogFrame } from "../../components/foundation/DialogFrame";
import { Button } from "../../components/ui/button";
import { useI18n } from "../../i18n/I18nProvider";
import type { Tenant, TenantCreateParams } from "../../types";

export function TenantSwitcher({
  activeTenant,
  busy,
  expanded = false,
  loading,
  onOpen,
  open,
}: {
  activeTenant: Tenant | null;
  busy: boolean;
  expanded?: boolean;
  loading: boolean;
  onOpen: () => void;
  open: boolean;
}) {
  const { t } = useI18n();
  const activeTenantName = activeTenant?.name ?? (loading ? t("tenant.loading") : t("tenant.label"));
  const triggerLabel = t("tenant.open");

  return (
    <button
      aria-label={triggerLabel}
      className={clsx(
        "flex h-10 min-w-0 items-center rounded-xl border transition-all active:scale-95",
        expanded ? "w-full justify-start gap-3 px-3" : "size-10 justify-center",
        open
          ? "border-theme-nav-active-border bg-theme-nav-active text-theme-nav-active-fg"
          : "border-transparent text-on-surface-variant/75 hover:border-theme-nav-active-border hover:bg-theme-nav-hover hover:text-theme-nav-active-fg",
      )}
      onClick={onOpen}
      title={activeTenant ? `${triggerLabel}: ${activeTenant.name}` : triggerLabel}
      type="button"
    >
      <span className="relative grid size-5 shrink-0 place-items-center">
        <Building2 size={19} aria-hidden="true" />
        {busy || loading ? (
          <span className="absolute -right-1 -top-1 grid size-3.5 place-items-center rounded-full border border-theme-nav bg-theme-card text-primary">
            <Loader2 size={10} className="animate-spin" aria-hidden="true" />
          </span>
        ) : null}
      </span>
      {expanded ? (
        <span className="min-w-0 truncate text-left text-body-sm font-medium" data-side-rail-label="">
          {activeTenantName}
        </span>
      ) : null}
    </button>
  );
}

export function TenantSwitcherDialog({
  activeTenant,
  busy,
  error,
  loading,
  onClose,
  onCreateTenant,
  onSwitchTenant,
  tenants,
}: {
  activeTenant: Tenant | null;
  busy: boolean;
  error?: string | null;
  loading: boolean;
  onClose: () => void;
  onCreateTenant: (params: TenantCreateParams) => Promise<unknown>;
  onSwitchTenant: (tenantId: string) => Promise<unknown>;
  tenants: Tenant[];
}) {
  const { t } = useI18n();
  const formId = useId();
  const selectId = useId();
  const nameInputRef = useRef<HTMLInputElement>(null);
  const selectRef = useRef<HTMLSelectElement>(null);
  const [localError, setLocalError] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [slug, setSlug] = useState("");

  const disabled = loading || busy;
  const activeTenantId = activeTenant?.id ?? "";
  const displayedTenants = tenants.length > 0 ? tenants : activeTenant ? [activeTenant] : [];
  const errorText = localError ?? error ?? null;

  async function handleSwitch(nextTenantId: string) {
    if (!nextTenantId || nextTenantId === activeTenantId) {
      return;
    }

    setLocalError(null);
    try {
      await onSwitchTenant(nextTenantId);
    } catch (nextError) {
      setLocalError(errorMessage(nextError));
    }
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const trimmedName = name.trim();
    const trimmedSlug = slug.trim();
    if (!trimmedName) {
      setLocalError(t("tenant.error.nameRequired"));
      return;
    }

    setLocalError(null);
    try {
      await onCreateTenant({
        name: trimmedName,
        set_active: true,
        slug: trimmedSlug || null,
      });
      setName("");
      setSlug("");
      onClose();
    } catch (nextError) {
      setLocalError(errorMessage(nextError));
    }
  }

  return (
    <DialogFrame
      busy={busy}
      closeLabel={t("common.close")}
      icon={<Building2 size={18} />}
      initialFocusRef={selectRef}
      onClose={onClose}
      overlayClassName="z-40 px-6 py-8"
      size="md"
      title={t("tenant.open")}
    >
      <div className="grid gap-5">
        <section className="grid gap-2">
          <label className="text-body-sm font-semibold text-on-surface" htmlFor={selectId}>
            {t("tenant.switchAria")}
          </label>
          <div className="flex items-center gap-2 rounded-lg border border-theme-control-border bg-theme-control px-3">
            <select
              aria-label={t("tenant.switchAria")}
              className="h-10 min-w-0 flex-1 truncate bg-transparent font-semibold text-on-surface outline-none disabled:cursor-not-allowed disabled:text-on-surface-variant"
              disabled={disabled || displayedTenants.length === 0}
              id={selectId}
              onChange={(event) => void handleSwitch(event.target.value)}
              ref={selectRef}
              value={activeTenantId}
            >
              {loading && displayedTenants.length === 0 ? <option value="">{t("tenant.loading")}</option> : null}
              {displayedTenants.map((tenant) => (
                <option key={tenant.id} value={tenant.id}>
                  {tenant.name}
                </option>
              ))}
            </select>
            {busy ? <Loader2 size={15} className="animate-spin text-outline" aria-hidden="true" /> : null}
          </div>
        </section>

        <form className="grid gap-4 border-t border-theme-card-border pt-5" id={formId} onSubmit={(event) => void handleSubmit(event)}>
          <h3 className="text-body-md font-semibold text-on-surface">{t("tenant.createTitle")}</h3>
          <Field label={t("tenant.name")} required>
            <input
              className="h-9 rounded-lg border border-theme-control-border bg-theme-control px-3 text-body-sm text-on-surface outline-none focus:border-theme-focus"
              disabled={busy}
              onChange={(event) => setName(event.target.value)}
              ref={nameInputRef}
              value={name}
            />
          </Field>
          <Field label={t("tenant.slug")}>
            <input
              className="h-9 rounded-lg border border-theme-control-border bg-theme-control px-3 font-mono text-body-sm text-on-surface outline-none focus:border-theme-focus"
              disabled={busy}
              onChange={(event) => setSlug(event.target.value)}
              placeholder={t("tenant.slugPlaceholder")}
              value={slug}
            />
          </Field>
          <div className="flex justify-end">
            <Button disabled={busy || !name.trim()} form={formId} type="submit">
              {busy ? t("tenant.creating") : t("tenant.create")}
            </Button>
          </div>
        </form>

        {errorText ? (
          <p className="rounded-lg border border-status-danger/30 bg-status-danger/10 px-3 py-2 text-body-sm text-status-danger" role="alert">
            {errorText}
          </p>
        ) : null}
      </div>
    </DialogFrame>
  );
}

function Field({ children, label, required = false }: { children: ReactNode; label: string; required?: boolean }) {
  return (
    <label className="grid gap-1.5 text-body-sm font-semibold text-on-surface">
      <span>
        {label}
        {required ? <span className="text-status-danger"> *</span> : null}
      </span>
      {children}
    </label>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
