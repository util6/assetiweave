import { Brain, Save } from "lucide-react";
import { useId, useRef, useState, type FormEvent, type ReactNode } from "react";
import { DialogFrame } from "../foundation/DialogFrame";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { useI18n } from "../../i18n/I18nProvider";
import { controlRecipe } from "../../theme/recipes";
import type { MemoryItemDetail, MemoryItemKind, MemoryScope } from "../../types/memory";

export type MemoryEditorMode = "create" | "edit" | "accept";

export interface MemoryEditorValues {
  confidence: number | null;
  content_markdown: string;
  kind: MemoryItemKind;
  scope: MemoryScope;
  title: string;
}

export function MemoryItemEditorDialog({
  busy,
  detail,
  error,
  mode,
  onClose,
  onSubmit,
}: {
  busy: boolean;
  detail: MemoryItemDetail | null;
  error: string | null;
  mode: MemoryEditorMode;
  onClose: () => void;
  onSubmit: (values: MemoryEditorValues) => void;
}) {
  const { t } = useI18n();
  const formId = useId();
  const titleInputRef = useRef<HTMLInputElement | null>(null);
  const item = detail?.item;
  const [kind, setKind] = useState<MemoryItemKind>(item?.kind ?? "context");
  const [title, setTitle] = useState(item?.title ?? "");
  const [content, setContent] = useState(item?.content_markdown ?? "");
  const [confidence, setConfidence] = useState(item?.confidence?.toString() ?? "");
  const [appId, setAppId] = useState(item?.scope.app_id ?? "");
  const [sourceId, setSourceId] = useState(item?.scope.source_id ?? "");
  const [projectPath, setProjectPath] = useState(item?.scope.project_path ?? "");
  const [sessionId, setSessionId] = useState(item?.scope.session_id ?? "");

  const dialogTitle =
    mode === "create"
      ? t("memory.dialog.createTitle")
      : mode === "accept"
        ? t("memory.dialog.acceptTitle")
        : t("memory.dialog.editTitle");
  const submitLabel = mode === "create" ? t("memory.dialog.create") : mode === "accept" ? t("memory.dialog.accept") : t("common.save");

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const parsedConfidence = confidence.trim() === "" ? null : Number(confidence);
    if (parsedConfidence !== null && (!Number.isFinite(parsedConfidence) || parsedConfidence < 0 || parsedConfidence > 1)) {
      return;
    }
    onSubmit({
      confidence: parsedConfidence,
      content_markdown: content.trim(),
      kind,
      scope: {
        app_id: optionalValue(appId),
        project_path: optionalValue(projectPath),
        session_id: optionalValue(sessionId),
        source_id: optionalValue(sourceId),
      },
      title: title.trim(),
    });
  }

  return (
    <DialogFrame
      busy={busy}
      closeLabel={t("common.close")}
      description={t("memory.field.optionalHint")}
      footer={
        <>
          <Button disabled={busy} onClick={onClose} type="button" variant="outline">
            {t("common.cancel")}
          </Button>
          <Button disabled={busy} form={formId} type="submit">
            <Save size={16} />
            {submitLabel}
          </Button>
        </>
      }
      icon={<Brain size={18} />}
      initialFocusRef={titleInputRef}
      onClose={onClose}
      size="lg"
      title={dialogTitle}
    >
      <form className="grid gap-4" id={formId} onSubmit={handleSubmit}>
        {error ? (
          <div className="rounded-lg border border-status-remove/35 bg-status-remove/10 px-3 py-2 text-body-sm text-status-remove" role="alert">
            {error}
          </div>
        ) : null}

        <div className="grid gap-4 sm:grid-cols-[minmax(10rem,0.35fr)_minmax(0,1fr)]">
          <MemoryField label={t("memory.field.kind")}>
            <select
              aria-label={t("memory.field.kind")}
              className={controlRecipe({ variant: "select" })}
              disabled={busy}
              onChange={(event) => setKind(event.target.value as MemoryItemKind)}
              value={kind}
            >
              <option value="preference">{t("memory.kind.preference")}</option>
              <option value="decision">{t("memory.kind.decision")}</option>
              <option value="method">{t("memory.kind.method")}</option>
              <option value="context">{t("memory.kind.context")}</option>
              <option value="follow_up">{t("memory.kind.follow_up")}</option>
            </select>
          </MemoryField>
          <MemoryField label={t("memory.field.title")}>
            <Input
              aria-label={t("memory.field.title")}
              disabled={busy}
              maxLength={240}
              onChange={(event) => setTitle(event.target.value)}
              ref={titleInputRef}
              required
              value={title}
            />
          </MemoryField>
        </div>

        <MemoryField label={t("memory.field.content")}>
          <textarea
            aria-label={t("memory.field.content")}
            className={`${controlRecipe({ variant: "textarea" })} min-h-44 w-full resize-y leading-6`}
            disabled={busy}
            maxLength={65536}
            onChange={(event) => setContent(event.target.value)}
            required
            value={content}
          />
        </MemoryField>

        <MemoryField label={t("memory.field.confidence")}>
          <Input
            aria-label={t("memory.field.confidence")}
            disabled={busy}
            max="1"
            min="0"
            onChange={(event) => setConfidence(event.target.value)}
            placeholder="0.00 – 1.00"
            step="0.01"
            type="number"
            value={confidence}
          />
        </MemoryField>

        <div className="grid gap-4 sm:grid-cols-2">
          <MemoryField label={t("memory.field.appId")}>
            <Input aria-label={t("memory.field.appId")} disabled={busy} onChange={(event) => setAppId(event.target.value)} value={appId} />
          </MemoryField>
          <MemoryField label={t("memory.field.sourceId")}>
            <Input
              aria-label={t("memory.field.sourceId")}
              disabled={busy}
              onChange={(event) => setSourceId(event.target.value)}
              value={sourceId}
            />
          </MemoryField>
          <MemoryField label={t("memory.field.projectPath")}>
            <Input
              aria-label={t("memory.field.projectPath")}
              disabled={busy}
              onChange={(event) => setProjectPath(event.target.value)}
              value={projectPath}
            />
          </MemoryField>
          <MemoryField label={t("memory.field.sessionId")}>
            <Input
              aria-label={t("memory.field.sessionId")}
              disabled={busy}
              onChange={(event) => setSessionId(event.target.value)}
              value={sessionId}
            />
          </MemoryField>
        </div>
      </form>
    </DialogFrame>
  );
}

function MemoryField({ children, label }: { children: ReactNode; label: string }) {
  return (
    <label className="grid min-w-0 gap-2 text-body-sm font-semibold text-on-surface">
      <span className="text-label-caps uppercase text-outline">{label}</span>
      {children}
    </label>
  );
}

function optionalValue(value: string) {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}
