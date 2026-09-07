import { FolderCog, Save } from "lucide-react";
import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { Controller, useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useI18n } from "../../i18n/I18nProvider";
import type { SkillBackupTaskSnapshot } from "../../services/catalog";
import type { Source } from "../../types";
import {
  deriveSourceName,
  type SourceImportFormValues,
} from "../../utils/sourceImport";
import { abbreviateHomePath } from "../../utils/path";
import {
  isSkillBackupRunning,
  SkillBackupButtonContent,
  SkillBackupInlineProgress,
} from "../backup/SkillBackupProgress";
import { PathPickerInput } from "../common/PathPickerInput";
import { DialogFrame } from "../foundation/DialogFrame";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Switch } from "../ui/switch";
import { sourceFormSchema, type SourceFormValues } from "./sourceFormSchema";

export function SourceEditDialog({
  backupAssetIds = [],
  backupAssetCount = 0,
  backupTask,
  busy,
  onBackup,
  onClose,
  onNotifyError,
  onPickRootPath,
  onSubmit,
  source,
}: {
  backupAssetIds?: string[];
  backupAssetCount?: number;
  backupTask?: SkillBackupTaskSnapshot | null;
  busy: boolean;
  onBackup?: () => Promise<void>;
  onClose: () => void;
  onNotifyError: (message: string) => void;
  onPickRootPath: () => Promise<string | null>;
  onSubmit: (source: Source) => Promise<void>;
  source: Source | null;
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
    defaultValues: sourceToFormValues(source),
    resolver: zodResolver(sourceFormSchema),
  });

  useEffect(() => {
    reset(sourceToFormValues(source));
    setPickingRootPath(false);
  }, [source, reset]);

  if (!source) {
    return null;
  }
  const currentSource = source;
  const backupActionLabel =
    backupAssetCount > 0
      ? t("backup.action.backupCount", { count: backupAssetCount })
      : t("backup.action.allInDirectory");

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
      await onSubmit({
        ...currentSource,
        enabled: values.enabled,
        exclude_globs: splitRuleLines(values.excludeGlobsText),
        include_globs: splitRuleLines(values.includeGlobsText),
        name: values.name.trim() || deriveSourceName(values.rootPath),
        priority: parsePriority(values.priority, currentSource.priority),
        root_path: values.rootPath.trim(),
      });
    } catch (error) {
      onNotifyError(error instanceof Error ? error.message : String(error));
    }
  });

  const { ref: formRootPathRef, ...rootPathRegister } = register("rootPath");

  const footer = (
    <>
      <div className="max-[640px]:grid">
        {onBackup && (
          <>
            <Button
              className="max-[640px]:w-full"
              disabled={
                busy ||
                backupAssetCount === 0 ||
                isSkillBackupRunning(backupTask ?? null)
              }
              onClick={() => void onBackup()}
              type="button"
              variant="outline"
            >
              <SkillBackupButtonContent
                assetIds={backupAssetIds}
                defaultLabel={backupActionLabel}
                task={backupTask ?? null}
                t={t}
              />
            </Button>
            <SkillBackupInlineProgress
              assetIds={backupAssetIds}
              task={backupTask ?? null}
              t={t}
            />
          </>
        )}
      </div>
      <div className="flex items-center justify-end gap-2 max-[640px]:grid max-[640px]:grid-cols-2">
        <Button
          disabled={busy}
          onClick={onClose}
          type="button"
          variant="outline"
        >
          {t("common.cancel")}
        </Button>
        <Button disabled={busy} form={formId} type="submit">
          <Save size={16} />
          {busy ? t("source.edit.submitting") : t("source.edit.submit")}
        </Button>
      </div>
    </>
  );

  return (
    <DialogFrame
      busy={busy}
      closeLabel={t("common.close")}
      contentClassName="p-0"
      description={currentSource.name}
      footer={footer}
      footerClassName="justify-between max-[640px]:flex-col max-[640px]:items-stretch"
      icon={<FolderCog size={18} />}
      iconClassName="border-status-update/25 bg-status-update/15 text-status-update"
      initialFocusRef={rootPathInputRef}
      onClose={onClose}
      size="lg"
      title={t("source.edit.title")}
    >
      <form
        className="px-5 py-5"
        id={formId}
        onSubmit={(event) => void onFormSubmit(event)}
      >
        <div className="grid gap-4">
          <Field label={t("source.field.rootPath")} required>
            <PathPickerInput
              aria-describedby={errors.rootPath ? rootPathErrorId : undefined}
              aria-invalid={Boolean(errors.rootPath)}
              disabled={busy}
              onPick={() => void handlePickRootPath()}
              pickLabel={t("source.import.pickDirectory")}
              picking={pickingRootPath}
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
              <Input disabled={busy} {...register("name")} />
            </Field>
            <Field label={t("source.field.priority")}>
              <Input
                aria-describedby={errors.priority ? priorityErrorId : undefined}
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
                {...register("includeGlobsText")}
              />
            </Field>
            <Field label={t("source.field.excludeGlobs")}>
              <textarea
                className="min-h-28 w-full resize-y rounded-xl border border-theme-control-border bg-theme-control px-3 py-2 font-mono text-code-md text-on-surface outline-none transition-[background-color,border-color,box-shadow,color] duration-200 placeholder:text-outline focus:border-primary-strong/60 disabled:cursor-not-allowed disabled:opacity-50"
                disabled={busy}
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

function sourceToFormValues(source: Source | null): SourceImportFormValues {
  return {
    enabled: source?.enabled ?? true,
    excludeGlobsText: source?.exclude_globs.join("\n") ?? "",
    includeGlobsText: source?.include_globs.join("\n") ?? "",
    name: source?.name ?? "",
    priority: String(source?.priority ?? 0),
    rootPath: source?.root_path ? abbreviateHomePath(source.root_path) : "",
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
