import clsx from "clsx";
import { Info, X } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useI18n } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import type { MountDisplayState } from "../../utils/mountState";

const mountDisplayStates: MountDisplayState[] = [
  "mounted",
  "not_mounted",
  "conflict",
  "broken",
];

export function MountStatePill({
  compact = false,
  state,
}: {
  compact?: boolean;
  state: MountDisplayState;
}) {
  const { t } = useI18n();
  const [helpOpen, setHelpOpen] = useState(false);

  return (
    <>
      <button
        aria-label={t("mount.stateHelp.openAria", { status: t(`mount.display.${state}` as TranslationKey) })}
        className={clsx(
          "inline-flex shrink-0 items-center gap-1.5 rounded-md border font-bold transition-colors hover:border-current focus:outline-none focus:ring-2 focus:ring-primary/45 focus:ring-offset-2 focus:ring-offset-background",
          compact ? "px-1.5 py-0.5 text-[10px]" : "px-2 py-0.5 text-[10px]",
          mountStatePillClass(state),
        )}
        onClick={(event) => {
          event.stopPropagation();
          setHelpOpen(true);
        }}
        title={t("mount.stateHelp.open")}
        type="button"
      >
        <span className={clsx("size-1.5 shrink-0 rounded-full", mountStateDotClass(state))} aria-hidden="true" />
        {t(`mount.display.${state}` as TranslationKey)}
        <Info className={compact ? "size-3" : "size-3.5"} aria-hidden="true" />
      </button>

      <MountStateHelpDialog currentState={state} onClose={() => setHelpOpen(false)} open={helpOpen} />
    </>
  );
}

export function mountStatePillClass(state: MountDisplayState) {
  if (state === "mounted") return "border-status-create/35 bg-status-create/15 text-status-create";
  if (state === "conflict") return "border-status-remove/45 bg-status-remove/12 text-status-remove";
  if (state === "broken") return "border-status-remove/45 bg-status-remove/12 text-status-remove";
  return "border-theme-control-border bg-theme-control-hover text-on-surface-variant";
}

function mountStateDotClass(state: MountDisplayState) {
  if (state === "mounted") return "bg-status-create";
  if (state === "conflict" || state === "broken") return "bg-status-remove";
  return "bg-outline";
}

function MountStateHelpDialog({
  currentState,
  onClose,
  open,
}: {
  currentState: MountDisplayState;
  onClose: () => void;
  open: boolean;
}) {
  const { t } = useI18n();
  const titleId = useId();
  const closeButtonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) {
      return;
    }

    closeButtonRef.current?.focus();

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onClose();
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose, open]);

  if (!open) {
    return null;
  }

  return createPortal(
    <div
      className="fixed inset-0 z-[70] grid place-items-center bg-background/72 px-4 py-8 backdrop-blur-sm"
      onClick={(event) => {
        event.stopPropagation();
        onClose();
      }}
    >
      <section
        aria-labelledby={titleId}
        aria-modal="true"
        className="flex max-h-[calc(100vh-64px)] w-full max-w-3xl flex-col overflow-hidden rounded-xl border border-theme-card-border bg-theme-card shadow-[0_24px_72px_rgb(var(--theme-panel-shadow)/0.34)]"
        onClick={(event) => event.stopPropagation()}
        role="dialog"
      >
        <header className="flex shrink-0 items-start justify-between gap-4 border-b border-theme-card-border bg-theme-card-header/70 px-5 py-4">
          <div className="min-w-0">
            <div className="flex min-w-0 items-center gap-2">
              <Info className="size-5 shrink-0 text-primary" aria-hidden="true" />
              <h2 className="truncate text-h2 text-on-surface" id={titleId}>
                {t("mount.stateHelp.title")}
              </h2>
            </div>
            <p className="mt-1 text-body-sm text-on-surface-variant">{t("mount.stateHelp.description")}</p>
          </div>
          <button
            aria-label={t("mount.stateHelp.close")}
            className="grid size-8 shrink-0 place-items-center rounded-lg text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-on-surface"
            onClick={onClose}
            ref={closeButtonRef}
            title={t("mount.stateHelp.close")}
            type="button"
          >
            <X size={18} />
          </button>
        </header>

        <div className="min-h-0 overflow-y-auto px-5 py-4">
          <div className="grid gap-3">
            {mountDisplayStates.map((state) => {
              const active = state === currentState;
              return (
                <article
                  className={clsx(
                    "rounded-xl border bg-theme-control/65 p-3 transition-colors",
                    active ? "border-primary/60 ring-1 ring-primary/20" : "border-theme-control-border",
                  )}
                  key={state}
                >
                  <div className="flex min-w-0 flex-wrap items-center gap-2">
                    <span className={clsx("size-2 rounded-full", mountStateDotClass(state))} aria-hidden="true" />
                    <h3 className="text-body-md font-bold text-on-surface">{t(`mount.display.${state}` as TranslationKey)}</h3>
                    {active && (
                      <span className="rounded-md border border-primary/40 bg-primary/10 px-2 py-0.5 text-[10px] font-bold text-primary">
                        {t("mount.stateHelp.current")}
                      </span>
                    )}
                  </div>

                  <div className="mt-3 grid gap-2 text-body-sm text-on-surface-variant">
                    <p>
                      <span className="font-bold text-on-surface">{t("mount.stateHelp.meaning")}</span>
                      <span className="ml-2">{t(`mount.stateHelp.${state}.meaning` as TranslationKey)}</span>
                    </p>
                    <p>
                      <span className="font-bold text-on-surface">{t("mount.stateHelp.action")}</span>
                      <span className="ml-2">{t(`mount.stateHelp.${state}.action` as TranslationKey)}</span>
                    </p>
                  </div>
                </article>
              );
            })}
          </div>
        </div>
      </section>
    </div>,
    document.body,
  );
}
