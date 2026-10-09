import clsx from "clsx";
import { FolderPlus, GitBranch, GitFork, Loader2 } from "lucide-react";
import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { Controller, useForm, useWatch } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { useI18n } from "../../i18n/I18nProvider";
import { probeSourcePath } from "../../services/catalog";
import type { SourceInput, SourceProbeResult } from "../../types";
import {
  buildCandidateSourceInputs,
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
  onSubmit: (source: SourceInput | SourceInput[]) => Promise<void>;
  open: boolean;
  suggestedPriority: number;
}) {
  const { t } = useI18n();
  const rootPathErrorId = useId();
  const priorityErrorId = useId();
  const formId = useId();
  const rootPathInputRef = useRef<HTMLInputElement>(null);
  const [pickingRootPath, setPickingRootPath] = useState(false);
  const [probing, setProbing] = useState(false);
  const [probeResult, setProbeResult] = useState<SourceProbeResult | null>(
    null,
  );
  const [importMode, setImportMode] = useState<"split" | "single">("split");
  const [selectedCandidatePaths, setSelectedCandidatePaths] = useState<
    Set<string>
  >(new Set());

  const {
    control,
    formState: { errors },
    handleSubmit,
    register,
    reset,
    setValue,
    watch,
  } = useForm<SourceFormValues>({
    defaultValues: createInitialValues(suggestedPriority),
    resolver: zodResolver(sourceFormSchema),
  });

  const prevOpenRef = useRef(false);
  useEffect(() => {
    if (open && !prevOpenRef.current) {
      reset(createInitialValues(suggestedPriority));
      setPickingRootPath(false);
      setProbeResult(null);
      setImportMode("split");
      setSelectedCandidatePaths(new Set());
    }
    prevOpenRef.current = open;
  }, [open, suggestedPriority, reset]);

  const debounceTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const runProbeDebounced = (rawPath: string, immediate = false) => {
    if (debounceTimerRef.current) {
      clearTimeout(debounceTimerRef.current);
    }
    const trimmed = rawPath.trim();
    if (!trimmed) {
      setProbeResult(null);
      return;
    }

    const execute = async () => {
      setProbing(true);
      try {
        const result = await probeSourcePath(trimmed);
        setProbeResult(result);
        if (result.is_container && result.candidates.length > 0) {
          setImportMode("split");
          setSelectedCandidatePaths(
            new Set(result.candidates.map((candidate) => candidate.path)),
          );
        } else {
          setImportMode("single");
          setSelectedCandidatePaths(new Set());
        }
      } catch {
        setProbeResult(null);
      } finally {
        setProbing(false);
      }
    };

    if (immediate) {
      void execute();
    } else {
      debounceTimerRef.current = setTimeout(() => {
        void execute();
      }, 50);
    }
  };

  useEffect(() => {
    const subscription = watch((value, { name }) => {
      if (name === "rootPath") {
        const raw = value.rootPath ?? "";
        runProbeDebounced(typeof raw === "string" ? raw : "");
      }
    });
    return () => {
      subscription.unsubscribe();
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
      }
    };
  }, []);

  if (!open) {
    return null;
  }

  async function handlePickRootPath() {
    setPickingRootPath(true);
    try {
      const selectedPath = await onPickRootPath();
      if (selectedPath) {
        const nextPath = abbreviateHomePath(selectedPath);
        setValue("rootPath", nextPath, {
          shouldDirty: true,
          shouldValidate: true,
        });
        runProbeDebounced(nextPath, true);
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
      if (probeResult?.is_container && importMode === "split") {
        const selected = probeResult.candidates.filter((candidate) =>
          selectedCandidatePaths.has(candidate.path),
        );
        if (selected.length === 0) {
          onNotifyError(t("source.import.probe.noCandidateSelected"));
          return;
        }
        const inputs = selected.map((candidate, idx) =>
          buildCandidateSourceInputs(candidate, values, idx),
        );
        await onSubmit(inputs);
      } else {
        const overrideOrigin = probeResult?.default_source_origin;
        await onSubmit(buildImportSourceInput(values, overrideOrigin));
      }
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
            {busy
              ? t("source.import.submitting")
              : importMode === "split" && selectedCandidatePaths.size > 0
                ? `${t("source.import.submit")} (${selectedCandidatePaths.size})`
                : t("source.import.submit")}
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
              aria-describedby={errors.rootPath ? rootPathErrorId : undefined}
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
              onChange={(event) => {
                void rootPathRegister.onChange(event);
                runProbeDebounced(event.target.value);
              }}
            />
            {errors.rootPath && (
              <FieldError id={rootPathErrorId}>
                {t("source.import.error.rootPathRequired")}
              </FieldError>
            )}

            {probing && (
              <div className="mt-2 flex items-center gap-2 text-body-xs text-on-surface-variant">
                <Loader2 className="size-3.5 animate-spin text-primary" />
                <span>{t("source.import.probe.probing")}</span>
              </div>
            )}

            {!probing &&
              probeResult &&
              !probeResult.is_container &&
              probeResult.default_source_origin === "git_repo" && (
                <div className="mt-2 flex items-center gap-2 rounded-xl border border-primary/20 bg-primary/10 px-3 py-1.5 text-body-xs text-primary">
                  <GitBranch className="size-3.5" />
                  <span>{t("source.import.probe.singleGitFound")}</span>
                </div>
              )}

            {!probing &&
              probeResult?.is_container &&
              probeResult.candidates.length > 0 && (
                <div className="mt-3 rounded-2xl border border-theme-card-border bg-theme-card/65 p-4">
                  <div className="flex items-start justify-between gap-3">
                    <div className="space-y-1">
                      <div className="flex items-center gap-2">
                        <GitFork className="size-4 text-primary" />
                        <span className="text-body-sm font-medium text-on-surface">
                          {t("source.import.probe.multiRepoTitle")}
                        </span>
                        <span className="rounded-full bg-primary/10 px-2 py-0.5 font-mono text-label-xs text-primary">
                          {probeResult.candidates.length} Repos
                        </span>
                      </div>
                      <p className="text-body-xs text-on-surface-variant">
                        {t("source.import.probe.multiRepoDesc", {
                          count: probeResult.candidates.length,
                        })}
                      </p>
                    </div>
                  </div>

                  <div className="mt-3 flex items-center gap-2">
                    <button
                      className={clsx(
                        "rounded-full border px-3 py-1.5 text-label-sm font-medium transition-all duration-200",
                        importMode === "split"
                          ? "border-primary/40 bg-primary/15 text-primary shadow-sm"
                          : "border-theme-control-border bg-theme-control/60 text-on-surface-variant hover:text-on-surface",
                      )}
                      onClick={() => setImportMode("split")}
                      type="button"
                    >
                      {t("source.import.probe.modeSplit")}
                    </button>
                    <button
                      className={clsx(
                        "rounded-full border px-3 py-1.5 text-label-sm font-medium transition-all duration-200",
                        importMode === "single"
                          ? "border-primary/40 bg-primary/15 text-primary shadow-sm"
                          : "border-theme-control-border bg-theme-control/60 text-on-surface-variant hover:text-on-surface",
                      )}
                      onClick={() => setImportMode("single")}
                      type="button"
                    >
                      {t("source.import.probe.modeSingle")}
                    </button>
                  </div>

                  {importMode === "split" && (
                    <div className="mt-3 space-y-2">
                      <div className="flex items-center justify-between px-1 text-body-xs text-on-surface-variant">
                        <span>{t("source.import.probe.selectRepos")}</span>
                        <button
                          className="cursor-pointer transition-colors hover:text-primary"
                          onClick={() => {
                            if (
                              selectedCandidatePaths.size ===
                              probeResult.candidates.length
                            ) {
                              setSelectedCandidatePaths(new Set());
                            } else {
                              setSelectedCandidatePaths(
                                new Set(
                                  probeResult.candidates.map(
                                    (candidate) => candidate.path,
                                  ),
                                ),
                              );
                            }
                          }}
                          type="button"
                        >
                          {selectedCandidatePaths.size ===
                          probeResult.candidates.length
                            ? t("source.import.probe.deselectAll")
                            : t("source.import.probe.selectAll")}
                        </button>
                      </div>
                      <div className="max-h-48 space-y-1.5 overflow-y-auto rounded-xl border border-theme-control-border/60 bg-theme-control/30 p-2">
                        {probeResult.candidates.map((candidate) => {
                          const isChecked = selectedCandidatePaths.has(
                            candidate.path,
                          );
                          return (
                            <label
                              className={clsx(
                                "flex cursor-pointer items-center justify-between gap-3 rounded-xl border px-3 py-2 transition-colors",
                                isChecked
                                  ? "border-theme-control-border bg-theme-control/90 text-on-surface"
                                  : "border-transparent bg-transparent text-on-surface-variant hover:bg-theme-control/40",
                              )}
                              key={candidate.path}
                            >
                              <div className="flex min-w-0 items-center gap-2.5">
                                <input
                                  checked={isChecked}
                                  className="size-4 rounded border-theme-control-border text-primary focus:ring-0"
                                  onChange={(event) => {
                                    const next = new Set(
                                      selectedCandidatePaths,
                                    );
                                    if (event.target.checked) {
                                      next.add(candidate.path);
                                    } else {
                                      next.delete(candidate.path);
                                    }
                                    setSelectedCandidatePaths(next);
                                  }}
                                  type="checkbox"
                                />
                                <div className="min-w-0">
                                  <div className="truncate font-mono text-body-sm font-medium">
                                    {candidate.name}
                                  </div>
                                  <div className="truncate font-mono text-label-xs text-outline">
                                    {candidate.relative_path}
                                  </div>
                                </div>
                              </div>
                              {candidate.skill_count > 0 && (
                                <span className="shrink-0 rounded-full border border-status-create/20 bg-status-create/10 px-2 py-0.5 text-label-xs text-status-create">
                                  {t("source.import.probe.skillsFound", {
                                    count: candidate.skill_count,
                                  })}
                                </span>
                              )}
                            </label>
                          );
                        })}
                      </div>
                    </div>
                  )}
                </div>
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
