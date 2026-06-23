import { Check, Database, FileJson, Folder, Loader2, UploadCloud } from "lucide-react";
import { useId, useMemo, useRef, useState, type FormEvent } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import type { ConversationRecordKind, ConversationSourceKind } from "../../types";
import { PathPickerInput } from "../common/PathPickerInput";
import { DialogFrame } from "../foundation/DialogFrame";
import { Button } from "../ui/button";
import { Input } from "../ui/input";

export interface ConversationImportFormValues {
  config_json: string | null;
  manifest_path: string;
  source_kind: ConversationSourceKind;
  source_location: string;
  source_name: string;
}

export type ConversationImportStep = "idle" | "validating" | "source" | "sync" | "failed";
type ImportableConversationSourceKind = Extract<ConversationSourceKind, "directory" | "file" | "sqlite" | "custom">;

export function ConversationImportDialog({
  busy = false,
  onClose,
  onImport,
  onPickManifest,
  onPickSourceLocation,
  recordKind,
  step = "idle",
}: {
  busy?: boolean;
  onClose: () => void;
  onImport: (values: ConversationImportFormValues) => Promise<void>;
  onPickManifest: () => Promise<string | null>;
  onPickSourceLocation: (kind: ConversationSourceKind) => Promise<string | null>;
  recordKind: ConversationRecordKind;
  step?: ConversationImportStep;
}) {
  const { t } = useI18n();
  const manifestInputRef = useRef<HTMLInputElement>(null);
  const sourceKindId = useId();
  const configId = useId();
  const [manifestPath, setManifestPath] = useState("");
  const [sourceName, setSourceName] = useState("");
  const [sourceKind, setSourceKind] = useState<ImportableConversationSourceKind>(
    recordKind === "web" ? "directory" : "directory",
  );
  const [sourceLocation, setSourceLocation] = useState("");
  const [configJson, setConfigJson] = useState("");
  const [error, setError] = useState<string | null>(null);
  const importDisabled = busy || !manifestPath.trim() || !sourceLocation.trim();
  const progressStep = importProgressStep(step);
  const sourcePickerLabel = sourceKind === "directory"
    ? t("conversation.import.pickSourceDirectory")
    : t("conversation.import.pickSourceFile");
  const sourceKindOptions = useMemo<ImportableConversationSourceKind[]>(
    () => ["directory", "file", "sqlite", "custom"],
    [],
  );

  async function handlePickManifest() {
    const selected = await onPickManifest();
    if (selected) {
      setManifestPath(selected);
      setError(null);
    }
  }

  async function handlePickSourceLocation() {
    const selected = await onPickSourceLocation(sourceKind);
    if (selected) {
      setSourceLocation(selected);
      setError(null);
    }
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!manifestPath.trim()) {
      setError(t("conversation.import.error.manifestRequired"));
      manifestInputRef.current?.focus();
      return;
    }
    if (!sourceLocation.trim()) {
      setError(t("conversation.import.error.locationRequired"));
      return;
    }
    setError(null);
    await onImport({
      config_json: configJson.trim() ? configJson : null,
      manifest_path: manifestPath.trim(),
      source_kind: sourceKind,
      source_location: sourceLocation.trim(),
      source_name: sourceName.trim(),
    });
  }

  return (
    <DialogFrame
      busy={busy}
      closeLabel={t("conversation.import.close")}
      description={t(
        recordKind === "web"
          ? "conversation.import.webDescription"
          : "conversation.import.sessionDescription",
      )}
      icon={<UploadCloud size={19} />}
      initialFocusRef={manifestInputRef}
      onClose={onClose}
      size="lg"
      title={t(recordKind === "web" ? "conversation.import.webTitle" : "conversation.import.sessionTitle")}
      footer={
        <>
          <Button disabled={busy} onClick={onClose} type="button" variant="ghost">
            {t("common.cancel")}
          </Button>
          <Button disabled={importDisabled} form="conversation-import-form" type="submit">
            {busy ? t("conversation.import.importing") : t("conversation.import.submit")}
          </Button>
        </>
      }
    >
      <form className="space-y-4" id="conversation-import-form" onSubmit={handleSubmit}>
        <div className="grid gap-4 md:grid-cols-[1fr_12rem]">
          <label className="space-y-2">
            <span className="text-label-caps uppercase text-outline">
              {t("conversation.import.manifestPath")}
            </span>
            <PathPickerInput
              aria-label={t("conversation.import.manifestPath")}
              disabled={busy}
              onChange={(event) => setManifestPath(event.target.value)}
              onPick={() => void handlePickManifest()}
              pickLabel={t("conversation.import.pickManifest")}
              placeholder="~/adapters/conversation-adapter.json"
              ref={manifestInputRef}
              value={manifestPath}
            />
          </label>
          <label className="space-y-2">
            <span className="text-label-caps uppercase text-outline">
              {t("conversation.import.sourceKind")}
            </span>
            <select
              aria-label={t("conversation.import.sourceKind")}
              className="h-10 w-full rounded-lg border border-theme-control-border bg-theme-control px-3 text-body-sm text-on-surface outline-none transition-colors focus:border-primary-strong/65"
              disabled={busy}
              id={sourceKindId}
              onChange={(event) => setSourceKind(event.target.value as ImportableConversationSourceKind)}
              value={sourceKind}
            >
              {sourceKindOptions.map((kind) => (
                <option key={kind} value={kind}>
                  {t(`conversation.import.sourceKind.${kind}` as TranslationKey)}
                </option>
              ))}
            </select>
          </label>
        </div>

        <label className="space-y-2">
          <span className="text-label-caps uppercase text-outline">
            {t("conversation.import.sourceLocation")}
          </span>
          <PathPickerInput
            aria-label={t("conversation.import.sourceLocation")}
            disabled={busy}
            onChange={(event) => setSourceLocation(event.target.value)}
            onPick={() => void handlePickSourceLocation()}
            pickLabel={sourcePickerLabel}
            placeholder={sourceKind === "sqlite" ? "~/Library/app/state.db" : "~/Downloads/conversation-export"}
            value={sourceLocation}
          />
        </label>

        <label className="space-y-2">
          <span className="text-label-caps uppercase text-outline">
            {t("conversation.import.sourceName")}
          </span>
          <Input
            aria-label={t("conversation.import.sourceName")}
            disabled={busy}
            onChange={(event) => setSourceName(event.target.value)}
            placeholder={t("conversation.import.sourceNamePlaceholder")}
            value={sourceName}
          />
        </label>

        <label className="space-y-2" htmlFor={configId}>
          <span className="text-label-caps uppercase text-outline">
            {t("conversation.import.configJson")}
          </span>
          <textarea
            aria-label={t("conversation.import.configJson")}
            className="min-h-20 w-full rounded-lg border border-theme-control-border bg-theme-control px-3 py-2 text-code-sm text-on-surface outline-none transition-colors placeholder:text-on-surface-muted focus:border-primary-strong/65"
            disabled={busy}
            id={configId}
            onChange={(event) => setConfigJson(event.target.value)}
            placeholder='{"workspace": "default"}'
            value={configJson}
          />
        </label>

        {error ? (
          <p className="rounded-lg border border-status-remove/40 bg-status-remove/10 px-3 py-2 text-body-sm text-status-remove">
            {error}
          </p>
        ) : null}

        <section aria-label={t("conversation.import.progressAria")} className="rounded-lg border border-theme-card-border bg-theme-control/50 p-3">
          <div className="grid gap-2 md:grid-cols-4">
            {(["validating", "source", "sync", "done"] as const).map((item, index) => (
              <ImportStep
                active={progressStep === index + 1 && busy}
                completed={progressStep > index + 1}
                failed={step === "failed" && progressStep === index + 1}
                key={item}
                label={t(`conversation.import.step.${item}`)}
              />
            ))}
          </div>
          <div
            aria-valuemax={4}
            aria-valuemin={1}
            aria-valuenow={progressStep}
            className="mt-3 h-2 overflow-hidden rounded-full bg-theme-card"
            role="progressbar"
          >
            <div
              className={`h-full rounded-full transition-[width] duration-300 ${
                step === "failed" ? "bg-status-remove" : "bg-status-update"
              }`}
              style={{ width: `${progressStep * 25}%` }}
            />
          </div>
        </section>
      </form>
    </DialogFrame>
  );
}

function ImportStep({
  active,
  completed,
  failed,
  label,
}: {
  active: boolean;
  completed: boolean;
  failed: boolean;
  label: string;
}) {
  return (
    <div className="flex min-w-0 items-center gap-2 text-body-sm text-on-surface-variant">
      <span
        className={`grid size-7 shrink-0 place-items-center rounded-lg border ${
          failed
            ? "border-status-remove/50 bg-status-remove/15 text-status-remove"
            : completed
              ? "border-status-create/50 bg-status-create/15 text-status-create"
              : active
                ? "border-status-update/50 bg-status-update/15 text-status-update"
                : "border-theme-control-border bg-theme-card text-on-surface-muted"
        }`}
      >
        {completed ? (
          <Check size={14} />
        ) : active ? (
          <Loader2 className="animate-spin" size={14} />
        ) : label.includes("SQLite") ? (
          <Database size={14} />
        ) : label.includes("插件") || label.includes("adapter") || label.includes("Adapter") ? (
          <FileJson size={14} />
        ) : (
          <Folder size={14} />
        )}
      </span>
      <span className="truncate">{label}</span>
    </div>
  );
}

function importProgressStep(step: ConversationImportStep) {
  if (step === "validating") return 1;
  if (step === "source") return 2;
  if (step === "sync") return 3;
  if (step === "failed") return 3;
  return 1;
}
