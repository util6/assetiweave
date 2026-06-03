import { FolderOpen, Save, X } from "lucide-react";
import { useEffect, useId, useRef, useState, type FormEvent, type ReactNode } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import type { Source } from "../../types";
import {
  deriveSourceName,
  hasSourceImportFormErrors,
  type SourceImportFormErrors,
  type SourceImportFormValues,
  validateSourceImportForm,
} from "../../utils/sourceImport";
import { DialogFrame as FoundationDialogFrame } from "../foundation/DialogFrame";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Switch } from "../ui/switch";

export function SourceEditDialog({
  busy,
  onClose,
  onNotifyError,
  onPickRootPath,
  onSubmit,
  source,
}: {
  busy: boolean;
  onClose: () => void;
  onNotifyError: (message: string) => void;
  onPickRootPath: () => Promise<string | null>;
  onSubmit: (source: Source) => Promise<void>;
  source: Source | null;
}) {
  const { t } = useI18n();
  const rootPathErrorId = useId();
  const priorityErrorId = useId();
  const rootPathInputRef = useRef<HTMLInputElement>(null);
  const [values, setValues] = useState<SourceImportFormValues>(() => sourceToFormValues(source));
  const [fieldErrors, setFieldErrors] = useState<SourceImportFormErrors>({});
  const [pickingRootPath, setPickingRootPath] = useState(false);

  useEffect(() => {
    setValues(sourceToFormValues(source));
    setFieldErrors({});
    setPickingRootPath(false);
    if (source) {
      window.setTimeout(() => rootPathInputRef.current?.focus(), 0);
    }
  }, [source]);

  if (!source) {
    return null;
  }
  const currentSource = source;

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

    await onSubmit({
      ...currentSource,
      enabled: values.enabled,
      exclude_globs: splitRuleLines(values.excludeGlobsText),
      include_globs: splitRuleLines(values.includeGlobsText),
      name: values.name.trim() || deriveSourceName(values.rootPath),
      priority: parsePriority(values.priority, currentSource.priority),
      root_path: values.rootPath.trim(),
    });
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
      className="flex max-h-[92vh] max-w-2xl flex-col"
      contentClassName="min-h-0 overflow-y-auto p-0"
      description={currentSource.name}
      headerActions={
        <Button
          aria-label={t("common.close")}
          className="text-on-surface-variant hover:text-on-surface"
          disabled={busy}
          onClick={onClose}
          size="icon"
          title={t("common.close")}
          type="button"
          variant="ghost"
        >
          <X size={17} />
        </Button>
      }
      onBackdropClick={busy ? undefined : onClose}
      title={t("source.edit.title")}
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
                  ref={rootPathInputRef}
                  value={values.rootPath}
                />
                <Button
                  aria-label={t("source.import.pickDirectory")}
                  disabled={busy || pickingRootPath}
                  onClick={() => void handlePickRootPath()}
                  size="icon"
                  title={t("source.import.pickDirectory")}
                  type="button"
                  variant="outline"
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
                <Input disabled={busy} onChange={(event) => updateValue("name", event.target.value)} value={values.name} />
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
                  value={values.includeGlobsText}
                />
              </Field>
              <Field label={t("source.field.excludeGlobs")}>
                <textarea
                  className="min-h-28 w-full resize-y rounded-lg border border-theme-control-border bg-theme-control px-3 py-2 font-mono text-code-md text-on-surface outline-none transition-colors placeholder:text-outline focus:border-primary-strong/60 disabled:cursor-not-allowed disabled:opacity-50"
                  disabled={busy}
                  onChange={(event) => updateValue("excludeGlobsText", event.target.value)}
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
              {t("common.cancel")}
            </Button>
            <Button disabled={busy} type="submit">
              <Save size={16} />
              {busy ? t("source.edit.submitting") : t("source.edit.submit")}
            </Button>
          </footer>
        </form>
    </FoundationDialogFrame>
  );
}

function sourceToFormValues(source: Source | null): SourceImportFormValues {
  return {
    enabled: source?.enabled ?? true,
    excludeGlobsText: source?.exclude_globs.join("\n") ?? "",
    includeGlobsText: source?.include_globs.join("\n") ?? "",
    name: source?.name ?? "",
    priority: String(source?.priority ?? 0),
    rootPath: source?.root_path ?? "",
  };
}

function splitRuleLines(value: string) {
  return value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

function parsePriority(value: string, fallback: number) {
  const priority = Number(value);
  return Number.isInteger(priority) ? priority : fallback;
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
