import { FolderPlus } from "lucide-react";
import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { Controller, useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useI18n } from "../../i18n/I18nProvider";
import type { SourceInput } from "../../types";
import {
  buildImportSourceInput,
  DEFAULT_SKILL_EXCLUDE_GLOBS,
  DEFAULT_SKILL_INCLUDE_GLOBS,
  type SourceImportFormValues,
} from "../../utils/sourceImport";
import { abbreviateHomePath } from "../../utils/path";
import { PathPickerInput } from "../common/PathPickerInput";
import { DialogFrame } from "../foundation/DialogFrame";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Switch } from "../ui/switch";
import { sourceFormSchema, type SourceFormValues } from "./sourceFormSchema";

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
  const formId = useId();
  const rootPathInputRef = useRef<HTMLInputElement>(null);
  const [pickingRootPath, setPickingRootPath] = useState(false);

  const {
    control,
    formState: { errors },
    handleSubmit,
    register,
    reset,
    setValue,
  } = useForm<SourceFormValues>({
    defaultValues: createInitialValues(suggestedPriority),
    resolver: zodResolver(sourceFormSchema),
  });

  useEffect(() => {
    if (!open) {
      return;
    }
    reset(createInitialValues(suggestedPriority));
    setPickingRootPath(false);
  }, [open, reset, suggestedPriority]);

  if (!open) {
    return null;
  }

  async function handlePickRootPath() {
    setPickingRootPath(true);
    try {
      const selectedPath = await onPickRootPath();
      if (selectedPath) {
        setValue("rootPath", abbreviateHomePath(selectedPath), {
          shouldDirty: true,
          shouldValidate: true,
        });
      }
    } catch (error) {
      onNotifyError(
        error instanceof Error
          ? error.message
          : t("source.import.error.pickDirectory"),
      );
    } finally {
      setPickingRootPath(false);
    }
  }

  const onFormSubmit = handleSubmit(async (values) => {
    try {
      await onSubmit(buildImportSourceInput(values));
      onClose();
    } catch (error) {
      onNotifyError(
        error instanceof Error
          ? error.message
          : t("source.import.error.submit"),
      );
    }
  });

  const { ref: formRootPathRef, ...rootPathRegister } = register("rootPath");

  return (
    <DialogFrame
      busy={busy}
      closeLabel={t("source.import.close")}
      contentClassName="p-0"
      footer={
        <>
          <Button
            disabled={busy}
            onClick={onClose}
            type="button"
            variant="outline"
          >
            {t("source.import.cancel")}
          </Button>
          <Button disabled={busy} form={formId} type="submit">
            {busy ? t("source.import.submitting") : t("source.import.submit")}
          </Button>
        </>
      }
      icon={<FolderPlus size={18} />}
      iconClassName="border-status-update/25 bg-status-update/15 text-status-update"
      initialFocusRef={rootPathInputRef}
      onClose={onClose}
      containerClassName="px-6 py-8"
      layer="base"
      size="lg"
      title={t("source.import.title")}
    >
      <form
        className="px-5 py-5"
        id={formId}
        onSubmit={(event) => void onFormSubmit(event)}
      >
        <div className="grid gap-4">
          <Field label={t("source.field.rootPath")} required>
            <PathPickerInput
              aria-describedby={
                errors.rootPath ? rootPathErrorId : undefined
              }
              aria-invalid={Boolean(errors.rootPath)}
              disabled={busy}
              onPick={() => void handlePickRootPath()}
              pickLabel={t("source.import.pickDirectory")}
              picking={pickingRootPath}
              placeholder={t("source.import.rootPathPlaceholder")}
              ref={(element) => {
                rootPathInputRef.current = element;
                formRootPathRef(element);
              }}
              {...rootPathRegister}
            />
            {errors.rootPath && (
              <FieldError id={rootPathErrorId}>
                {t("source.import.error.rootPathRequired")}
              </FieldError>
            )}
          </Field>

          <div className="grid grid-cols-[minmax(0,1fr)_8rem] gap-3 max-[720px]:grid-cols-1">
            <Field label={t("source.field.name")}>
              <Input
                disabled={busy}
                placeholder={t("source.import.namePlaceholder")}
                {...register("name")}
              />
            </Field>
            <Field label={t("source.field.priority")}>
              <Input
                aria-describedby={
                  errors.priority ? priorityErrorId : undefined
                }
                aria-invalid={Boolean(errors.priority)}
                disabled={busy}
                inputMode="numeric"
                {...register("priority")}
              />
              {errors.priority && (
                <FieldError id={priorityErrorId}>
                  {t("source.import.error.priorityInvalid")}
                </FieldError>
              )}
            </Field>
          </div>

          <div className="grid grid-cols-2 gap-3 max-[720px]:grid-cols-1">
            <Field label={t("source.field.includeGlobs")}>
              <textarea
                className="min-h-28 w-full resize-y rounded-xl border border-theme-control-border bg-theme-control px-3 py-2 font-mono text-code-md text-on-surface outline-none transition-[background-color,border-color,box-shadow,color] duration-200 placeholder:text-outline focus:border-primary-strong/60 disabled:cursor-not-allowed disabled:opacity-50"
                disabled={busy}
                placeholder={t("source.form.includePlaceholder")}
                {...register("includeGlobsText")}
              />
            </Field>
            <Field label={t("source.field.excludeGlobs")}>
              <textarea
                className="min-h-28 w-full resize-y rounded-xl border border-theme-control-border bg-theme-control px-3 py-2 font-mono text-code-md text-on-surface outline-none transition-[background-color,border-color,box-shadow,color] duration-200 placeholder:text-outline focus:border-primary-strong/60 disabled:cursor-not-allowed disabled:opacity-50"
                disabled={busy}
                placeholder={t("source.form.excludePlaceholder")}
                {...register("excludeGlobsText")}
              />
            </Field>
          </div>

          <div className="flex items-center justify-between gap-4 rounded-xl border border-theme-control-border bg-theme-control/70 px-3 py-3">
            <span className="text-body-sm text-on-surface">
              {t("source.field.enabled")}
            </span>
            <Controller
              control={control}
              name="enabled"
              render={({ field }) => (
                <Switch
                  aria-label={t("source.field.enabled")}
                  checked={field.value}
                  disabled={busy}
                  onCheckedChange={field.onChange}
                />
              )}
            />
          </div>
        </div>
      </form>
    </DialogFrame>
  );
}

function Field({
  children,
  label,
  required = false,
}: {
  children: ReactNode;
  label: string;
  required?: boolean;
}) {
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

function createInitialValues(
  suggestedPriority: number,
): SourceImportFormValues {
  return {
    enabled: true,
    excludeGlobsText: DEFAULT_SKILL_EXCLUDE_GLOBS.join("\n"),
    includeGlobsText: DEFAULT_SKILL_INCLUDE_GLOBS.join("\n"),
    name: "",
    priority: String(suggestedPriority),
    rootPath: "",
  };
}
