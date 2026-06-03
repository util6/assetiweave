import { FolderOpen, FolderPlus, X } from "lucide-react";
import { useEffect, useId, useRef, useState, type FormEvent, type ReactNode } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import type { SourceInput } from "../../types";
import {
  buildImportSourceInput,
  DEFAULT_SKILL_EXCLUDE_GLOBS,
  DEFAULT_SKILL_INCLUDE_GLOBS,
  hasSourceImportFormErrors,
  type SourceImportFormErrors,
  type SourceImportFormValues,
  validateSourceImportForm,
} from "../../utils/sourceImport";
import { DialogFrame as FoundationDialogFrame } from "../foundation/DialogFrame";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Switch } from "../ui/switch";

export function SourceImportDialog({
  busy,
  onClose,
  onNotifyError,
  onPickRootPath,
  onSubmit,
  open,
  suggestedPriority,
}: {
  busy: boolean;
  onClose: () => void;
  onNotifyError: (message: string) => void;
  onPickRootPath: () => Promise<string | null>;
  onSubmit: (source: SourceInput) => Promise<void>;
  open: boolean;
  suggestedPriority: number;
}) {
  const { t } = useI18n();
  const rootPathErrorId = useId();
  const priorityErrorId = useId();
  const rootPathInputRef = useRef<HTMLInputElement>(null);
  const [values, setValues] = useState<SourceImportFormValues>(() => createInitialValues(suggestedPriority));
  const [fieldErrors, setFieldErrors] = useState<SourceImportFormErrors>({});
  const [pickingRootPath, setPickingRootPath] = useState(false);

  useEffect(() => {
    if (!open) {
      return;
    }

    setValues(createInitialValues(suggestedPriority));
    setFieldErrors({});
    setPickingRootPath(false);
    window.setTimeout(() => rootPathInputRef.current?.focus(), 0);
  }, [open, suggestedPriority]);

  useEffect(() => {
    if (!open) {
      return;
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape" && !busy) {
        onClose();
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [busy, onClose, open]);

  if (!open) {
    return null;
  }

  function updateValue<Key extends keyof SourceImportFormValues>(key: Key, value: SourceImportFormValues[Key]) {
    setValues((currentValues) => ({ ...currentValues, [key]: value }));
    if (key === "rootPath" || key === "priority") {
      setFieldErrors((currentErrors) => ({ ...currentErrors, [key]: undefined }));
    }
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const errors = validateSourceImportForm(values);
    setFieldErrors(errors);
    if (hasSourceImportFormErrors(errors)) {
      return;
    }

    try {
      await onSubmit(buildImportSourceInput(values));
      onClose();
    } catch (error) {
      onNotifyError(error instanceof Error ? error.message : t("source.import.error.submit"));
    }
  }

  async function handlePickRootPath() {
    setPickingRootPath(true);
    try {
      const selectedPath = await onPickRootPath();
      if (selectedPath) {
        updateValue("rootPath", selectedPath);
      }
    } catch (error) {
      onNotifyError(error instanceof Error ? error.message : t("source.import.error.pickDirectory"));
    } finally {
      setPickingRootPath(false);
    }
  }

  return (
    <FoundationDialogFrame
      className="flex max-h-full max-w-2xl flex-col"
      contentClassName="min-h-0 overflow-y-auto p-0"
      headerActions={
        <Button
          aria-label={t("source.import.close")}
          className="text-on-surface-variant hover:text-on-surface"
          disabled={busy}
          onClick={onClose}
          size="icon"
          title={t("source.import.close")}
          type="button"
          variant="ghost"
        >
          <X size={18} />
        </Button>
      }
      headerClassName="h-16 shrink-0 items-center"
      icon={<FolderPlus size={18} />}
      iconClassName="border-status-update/25 bg-status-update/15 text-status-update"
      onBackdropClick={busy ? undefined : onClose}
      overlayClassName="z-40 px-6 py-8"
      title={t("source.import.title")}
    >
        <form className="px-5 py-5" onSubmit={(event) => void handleSubmit(event)}>
          <div className="grid gap-4">
            <Field label={t("source.field.rootPath")} required>
              <div className="flex gap-2">
                <Input
                  aria-describedby={fieldErrors.rootPath ? rootPathErrorId : undefined}
                  aria-invalid={Boolean(fieldErrors.rootPath)}
                  className="min-w-0 flex-1"
                  disabled={busy || pickingRootPath}
                  onChange={(event) => updateValue("rootPath", event.target.value)}
                  placeholder={t("source.import.rootPathPlaceholder")}
                  ref={rootPathInputRef}
                  value={values.rootPath}
                />
                <Button
                  aria-label={t("source.import.pickDirectory")}
                  disabled={busy || pickingRootPath}
                  onClick={() => void handlePickRootPath()}
                  title={t("source.import.pickDirectory")}
                  type="button"
                  variant="outline"
                  size="icon"
                >
                  <FolderOpen size={17} />
                </Button>
              </div>
              {fieldErrors.rootPath && (
                <FieldError id={rootPathErrorId}>{t("source.import.error.rootPathRequired")}</FieldError>
              )}
            </Field>

            <div className="grid grid-cols-[minmax(0,1fr)_8rem] gap-3 max-[720px]:grid-cols-1">
              <Field label={t("source.field.name")}>
                <Input
                  disabled={busy}
                  onChange={(event) => updateValue("name", event.target.value)}
                  placeholder={t("source.import.namePlaceholder")}
                  value={values.name}
                />
              </Field>
              <Field label={t("source.field.priority")}>
                <Input
                  aria-describedby={fieldErrors.priority ? priorityErrorId : undefined}
                  aria-invalid={Boolean(fieldErrors.priority)}
                  disabled={busy}
                  inputMode="numeric"
                  onChange={(event) => updateValue("priority", event.target.value)}
                  value={values.priority}
                />
                {fieldErrors.priority && (
                  <FieldError id={priorityErrorId}>{t("source.import.error.priorityInvalid")}</FieldError>
                )}
              </Field>
            </div>

            <div className="grid grid-cols-2 gap-3 max-[720px]:grid-cols-1">
              <Field label={t("source.field.includeGlobs")}>
                <textarea
                  className="min-h-28 w-full resize-y rounded-lg border border-theme-control-border bg-theme-control px-3 py-2 font-mono text-code-md text-on-surface outline-none transition-colors placeholder:text-outline focus:border-primary-strong/60 disabled:cursor-not-allowed disabled:opacity-50"
                  disabled={busy}
                  onChange={(event) => updateValue("includeGlobsText", event.target.value)}
                  placeholder={t("source.form.includePlaceholder")}
                  value={values.includeGlobsText}
                />
              </Field>
              <Field label={t("source.field.excludeGlobs")}>
                <textarea
                  className="min-h-28 w-full resize-y rounded-lg border border-theme-control-border bg-theme-control px-3 py-2 font-mono text-code-md text-on-surface outline-none transition-colors placeholder:text-outline focus:border-primary-strong/60 disabled:cursor-not-allowed disabled:opacity-50"
                  disabled={busy}
                  onChange={(event) => updateValue("excludeGlobsText", event.target.value)}
                  placeholder={t("source.form.excludePlaceholder")}
                  value={values.excludeGlobsText}
                />
              </Field>
            </div>

            <div className="flex items-center justify-between gap-4 rounded-xl border border-theme-control-border bg-theme-control/70 px-3 py-3">
              <span className="text-body-sm text-on-surface">{t("source.field.enabled")}</span>
              <Switch
                aria-label={t("source.field.enabled")}
                checked={values.enabled}
                disabled={busy}
                onCheckedChange={(checked) => updateValue("enabled", checked)}
              />
            </div>

          </div>

          <footer className="mt-5 flex justify-end gap-2 border-t border-theme-card-border pt-4">
            <Button disabled={busy} onClick={onClose} type="button" variant="outline">
              {t("source.import.cancel")}
            </Button>
            <Button disabled={busy} type="submit">
              {busy ? t("source.import.submitting") : t("source.import.submit")}
            </Button>
          </footer>
        </form>
    </FoundationDialogFrame>
  );
}

function Field({ children, label, required = false }: { children: ReactNode; label: string; required?: boolean }) {
  return (
    <label className="grid gap-1.5">
      <span className="text-body-sm font-medium text-on-surface-variant">
        {label}
        {required && <span className="text-status-remove"> *</span>}
      </span>
      {children}
    </label>
  );
}

function FieldError({ children, id }: { children: ReactNode; id: string }) {
  return (
    <span className="text-body-sm text-status-remove" id={id}>
      {children}
    </span>
  );
}

function createInitialValues(suggestedPriority: number): SourceImportFormValues {
  return {
    enabled: true,
    excludeGlobsText: DEFAULT_SKILL_EXCLUDE_GLOBS.join("\n"),
    includeGlobsText: DEFAULT_SKILL_INCLUDE_GLOBS.join("\n"),
    name: "",
    priority: String(suggestedPriority),
    rootPath: "",
  };
}
