import { Archive } from "lucide-react";
import { useEffect, useId, useRef, useState, type FormEvent, type ReactNode } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import {
  getSkillBackupSettings,
  selectTargetDirectory,
  updateSkillBackupSettings,
} from "../../services/catalog";
import type { SkillBackupSettings } from "../../types";
import { abbreviateHomePath } from "../../utils/path";
import { PathPickerInput } from "../common/PathPickerInput";
import { DialogFrame } from "../foundation/DialogFrame";
import { Button } from "../ui/button";

export function SkillBackupLibraryDialog({
  onClose,
  onNotifyError,
  onSaved,
  open,
}: {
  onClose: () => void;
  onNotifyError: (message: string) => void;
  onSaved?: (settings: SkillBackupSettings) => Promise<void> | void;
  open: boolean;
}) {
  const { t } = useI18n();
  const formId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const [settings, setSettings] = useState<SkillBackupSettings | null>(null);
  const [rootPath, setRootPath] = useState("");
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!open) {
      return;
    }

    setLoading(true);
    getSkillBackupSettings()
      .then((nextSettings) => {
        setSettings(nextSettings);
        setRootPath(abbreviateHomePath(nextSettings.root_path));
      })
      .catch((error) => onNotifyError(errorMessage(error)))
      .finally(() => setLoading(false));
  }, [onNotifyError, open]);

  if (!open) {
    return null;
  }

  async function handlePickDirectory() {
    setBusy(true);
    try {
      const selected = await selectTargetDirectory(t("backup.dialog.pickDirectory"));
      if (selected) {
        setRootPath(abbreviateHomePath(selected));
      }
    } catch (error) {
      onNotifyError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const trimmedRootPath = rootPath.trim();
    if (!trimmedRootPath) {
      onNotifyError(t("backup.error.rootPathRequired"));
      return;
    }

    setBusy(true);
    try {
      const nextSettings = await updateSkillBackupSettings(trimmedRootPath, true);
      setSettings(nextSettings);
      setRootPath(abbreviateHomePath(nextSettings.root_path));
      await onSaved?.(nextSettings);
      onClose();
    } catch (error) {
      onNotifyError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  const disabled = busy || loading;

  return (
    <DialogFrame
      busy={busy}
      closeLabel={t("backup.dialog.close")}
      contentClassName="p-0"
      footer={
        <>
          <Button disabled={busy} onClick={onClose} type="button" variant="outline">
            {t("common.cancel")}
          </Button>
          <Button disabled={disabled || !rootPath.trim()} form={formId} type="submit">
            {busy ? t("common.saving") : t("backup.action.save")}
          </Button>
        </>
      }
      icon={<Archive size={18} />}
      iconClassName="border-status-update/25 bg-status-update/15 text-status-update"
      initialFocusRef={inputRef}
      onClose={onClose}
      overlayClassName="z-40 px-6 py-8"
      size="lg"
      title={t("backup.dialog.title")}
    >
      <form className="px-5 py-5" id={formId} onSubmit={(event) => void handleSubmit(event)}>
        <div className="grid gap-4">
          <Field label={t("backup.field.rootPath")} required>
            <PathPickerInput
              disabled={disabled}
              inputClassName="font-mono"
              onChange={(event) => setRootPath(event.target.value)}
              onPick={() => void handlePickDirectory()}
              pickLabel={t("backup.dialog.pickDirectory")}
              placeholder={settings?.default_root_path ? abbreviateHomePath(settings.default_root_path) : "~/.assetiweave/library/skills"}
              ref={inputRef}
              value={rootPath}
            />
          </Field>

          {settings && (
            <div className="grid gap-2 rounded-lg border border-theme-control-border bg-theme-control/65 p-3">
              <ReadonlyRow label={t("backup.field.currentPath")} value={abbreviateHomePath(settings.expanded_root_path)} />
              <ReadonlyRow label={t("backup.field.defaultPath")} value={abbreviateHomePath(settings.default_root_path)} />
              <ReadonlyRow label={t("backup.field.mode")} value={settings.is_default_root ? t("backup.mode.default") : t("backup.mode.custom")} />
            </div>
          )}
        </div>

      </form>
    </DialogFrame>
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

function ReadonlyRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="grid grid-cols-[8rem_minmax(0,1fr)] gap-3 text-body-sm max-[640px]:grid-cols-1">
      <span className="text-on-surface-variant">{label}</span>
      <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-on-surface" title={value}>
        {value}
      </span>
    </div>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
