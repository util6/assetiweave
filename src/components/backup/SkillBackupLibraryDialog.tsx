import { Archive, FolderOpen, X } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import {
  getSkillBackupSettings,
  selectTargetDirectory,
  updateSkillBackupSettings,
} from "../../services/catalog";
import type { SkillBackupSettings } from "../../types";
import { abbreviateHomePath } from "../../utils/path";
import { DialogFrame as FoundationDialogFrame } from "../foundation/DialogFrame";
import { Button } from "../ui/button";
import { Input } from "../ui/input";

export function SkillBackupLibraryDialog({
  onClose,
  onNotifyError,
  onSaved,
  open,
}: {
  onClose: () => void;
  onNotifyError: (message: string) => void;
  onSaved?: () => Promise<void> | void;
  open: boolean;
}) {
  const { t } = useI18n();
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
        setRootPath(nextSettings.root_path);
        window.setTimeout(() => inputRef.current?.focus(), 0);
      })
      .catch((error) => onNotifyError(errorMessage(error)))
      .finally(() => setLoading(false));
  }, [onNotifyError, open]);

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

  async function handlePickDirectory() {
    setBusy(true);
    try {
      const selected = await selectTargetDirectory(t("backup.dialog.pickDirectory"));
      if (selected) {
        setRootPath(selected);
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
      setRootPath(nextSettings.root_path);
      await onSaved?.();
      onClose();
    } catch (error) {
      onNotifyError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  const disabled = busy || loading;

  return (
    <FoundationDialogFrame
      className="flex max-h-full max-w-2xl flex-col"
      contentClassName="min-h-0 overflow-y-auto p-0"
      headerActions={
        <Button
          aria-label={t("backup.dialog.close")}
          className="text-on-surface-variant hover:text-on-surface"
          disabled={busy}
          onClick={onClose}
          size="icon"
          title={t("backup.dialog.close")}
          type="button"
          variant="ghost"
        >
          <X size={18} />
        </Button>
      }
      headerClassName="h-16 shrink-0 items-center"
      icon={<Archive size={18} />}
      iconClassName="border-status-update/25 bg-status-update/15 text-status-update"
      onBackdropClick={busy ? undefined : onClose}
      overlayClassName="z-40 px-6 py-8"
      title={t("backup.dialog.title")}
    >
      <form className="px-5 py-5" onSubmit={(event) => void handleSubmit(event)}>
        <div className="grid gap-4">
          <Field label={t("backup.field.rootPath")} required>
            <div className="flex gap-2">
              <Input
                className="min-w-0 flex-1 font-mono"
                disabled={disabled}
                onChange={(event) => setRootPath(event.target.value)}
                placeholder={settings?.default_root_path ?? "~/.assetiweave/library/skills"}
                ref={inputRef}
                value={rootPath}
              />
              <Button
                aria-label={t("backup.dialog.pickDirectory")}
                disabled={disabled}
                onClick={() => void handlePickDirectory()}
                size="icon"
                title={t("backup.dialog.pickDirectory")}
                type="button"
                variant="outline"
              >
                <FolderOpen size={17} />
              </Button>
            </div>
          </Field>

          {settings && (
            <div className="grid gap-2 rounded-lg border border-theme-control-border bg-theme-control/65 p-3">
              <ReadonlyRow label={t("backup.field.currentPath")} value={abbreviateHomePath(settings.expanded_root_path)} />
              <ReadonlyRow label={t("backup.field.defaultPath")} value={abbreviateHomePath(settings.default_root_path)} />
              <ReadonlyRow label={t("backup.field.mode")} value={settings.is_default_root ? t("backup.mode.default") : t("backup.mode.custom")} />
            </div>
          )}
        </div>

        <footer className="mt-5 flex justify-end gap-2 border-t border-theme-card-border pt-4">
          <Button disabled={busy} onClick={onClose} type="button" variant="outline">
            {t("common.cancel")}
          </Button>
          <Button disabled={disabled || !rootPath.trim()} type="submit">
            {busy ? t("common.saving") : t("backup.action.save")}
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
