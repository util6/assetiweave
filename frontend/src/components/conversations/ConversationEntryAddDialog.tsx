import { Plus } from "lucide-react";
import { useMemo, useState, type FormEvent, type ReactNode } from "react";

import { PathPickerInput } from "../common/PathPickerInput";
import { DialogFrame } from "../foundation/DialogFrame";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import type { Translator } from "../../i18n/I18nProvider";
import type { ConversationRecordKind, ConversationSourceKind } from "../../types";

export interface ConversationEntryAddFormValues {
  configJson: string;
  location: string;
  pluginPath: string;
  sourceId: string;
  sourceKind: ConversationSourceKind;
  sourceName: string;
}

export function ConversationEntryAddDialog({
  busy,
  onClose,
  onPickLocation,
  onPickPlugin,
  onSubmit,
  recordKind,
  t,
}: {
  busy: boolean;
  onClose: () => void;
  onPickLocation: () => Promise<string | null>;
  onPickPlugin: () => Promise<string | null>;
  onSubmit: (values: ConversationEntryAddFormValues) => Promise<void>;
  recordKind: ConversationRecordKind;
  t: Translator;
}) {
  const [values, setValues] = useState<ConversationEntryAddFormValues>({
    configJson: "",
    location: "",
    pluginPath: "",
    sourceId: "",
    sourceKind: "directory",
    sourceName: "",
  });
  const [picking, setPicking] = useState<"plugin" | "location" | null>(null);

  const canSubmit = useMemo(
    () => Boolean(values.pluginPath.trim() && values.location.trim() && values.sourceName.trim()),
    [values.location, values.pluginPath, values.sourceName],
  );

  async function handlePickPlugin() {
    setPicking("plugin");
    try {
      const selected = await onPickPlugin();
      if (selected) {
        setValues((current) => ({ ...current, pluginPath: selected }));
      }
    } finally {
      setPicking(null);
    }
  }

  async function handlePickLocation() {
    setPicking("location");
    try {
      const selected = await onPickLocation();
      if (selected) {
        setValues((current) => ({ ...current, location: selected }));
      }
    } finally {
      setPicking(null);
    }
  }

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (!canSubmit || busy) {
      return;
    }
    await onSubmit(values);
  }

  return (
    <DialogFrame
      busy={busy}
      closeLabel={t("conversation.add.close")}
      footer={
        <>
          <Button disabled={busy} onClick={onClose} type="button" variant="outline">
            {t("common.cancel")}
          </Button>
          <Button disabled={busy || !canSubmit} form="conversation-entry-add-form" type="submit">
            {busy ? t("conversation.add.submitting") : t("conversation.add.submit")}
          </Button>
        </>
      }
      icon={<Plus size={18} />}
      onClose={onClose}
      size="lg"
      title={t(
        recordKind === "web"
          ? "conversation.add.title.web"
          : "conversation.add.title.session",
      )}
    >
      <form className="grid gap-4" id="conversation-entry-add-form" onSubmit={(event) => void handleSubmit(event)}>
        <Field label={t("conversation.add.field.plugin")} required>
          <PathPickerInput
            aria-label={t("conversation.add.field.plugin")}
            disabled={busy}
            onChange={(event) =>
              setValues((current) => ({ ...current, pluginPath: event.target.value }))
            }
            onPick={() => void handlePickPlugin()}
            pickLabel={t("conversation.add.pickPlugin")}
            picking={picking === "plugin"}
            value={values.pluginPath}
          />
        </Field>
        <div className="grid gap-4 md:grid-cols-[minmax(0,1fr)_12rem]">
          <Field label={t("conversation.add.field.sourceName")} required>
            <Input
              aria-label={t("conversation.add.field.sourceName")}
              disabled={busy}
              onChange={(event) =>
                setValues((current) => ({ ...current, sourceName: event.target.value }))
              }
              value={values.sourceName}
            />
          </Field>
          <Field label={t("conversation.add.field.sourceKind")}>
            <select
              aria-label={t("conversation.add.field.sourceKind")}
              className="h-10 w-full rounded-lg border border-theme-control-border bg-theme-control px-3 text-body-sm text-on-surface outline-none focus:border-primary/60"
              disabled={busy}
              onChange={(event) =>
                setValues((current) => ({
                  ...current,
                  sourceKind: event.target.value as ConversationSourceKind,
                }))
              }
              value={values.sourceKind}
            >
              {(["directory", "file", "sqlite", "live", "custom"] as ConversationSourceKind[]).map((kind) => (
                <option key={kind} value={kind}>
                  {t(`conversation.add.sourceKind.${kind}`)}
                </option>
              ))}
            </select>
          </Field>
        </div>
        <Field label={t("conversation.add.field.location")} required>
          <PathPickerInput
            aria-label={t("conversation.add.field.location")}
            disabled={busy}
            onChange={(event) =>
              setValues((current) => ({ ...current, location: event.target.value }))
            }
            onPick={() => void handlePickLocation()}
            pickLabel={t("conversation.add.pickLocation")}
            picking={picking === "location"}
            value={values.location}
          />
        </Field>
        <Field label={t("conversation.add.field.sourceId")}>
          <Input
            aria-label={t("conversation.add.field.sourceId")}
            disabled={busy}
            onChange={(event) =>
              setValues((current) => ({ ...current, sourceId: event.target.value }))
            }
            value={values.sourceId}
          />
        </Field>
        <Field label={t("conversation.add.field.configJson")}>
          <textarea
            aria-label={t("conversation.add.field.configJson")}
            className="min-h-24 w-full resize-y rounded-lg border border-theme-control-border bg-theme-control px-3 py-2 font-mono text-code-sm text-on-surface outline-none focus:border-primary/60"
            disabled={busy}
            onChange={(event) =>
              setValues((current) => ({ ...current, configJson: event.target.value }))
            }
            value={values.configJson}
          />
        </Field>
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
    <label className="grid gap-2 text-body-sm text-on-surface">
      <span className="font-medium">
        {label}
        {required && <span className="ml-1 text-danger">*</span>}
      </span>
      {children}
    </label>
  );
}
