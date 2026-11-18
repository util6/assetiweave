import clsx from "clsx";
import {
  CircleAlert,
  Download,
  ExternalLink,
  Info,
  PowerOff,
  ShieldCheck,
  Trash2,
  Wrench,
} from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import type { ConversationAdapterPackageCatalogEntry } from "../../services/conversations";
import { Badge } from "../foundation/Badge";
import { TruncatedText } from "../foundation/TruncatedText";
import { Button } from "../ui/button";
import {
  actionLabelKey,
  manifestDisplayPathForEntry,
  packageActionForEntry,
  packageLifecycleAction,
  statusBadgeTone,
  statusLabelKey,
} from "./conversationScriptResourceModel";

export function ConversationScriptResourceRow({
  changeDisabled,
  disabled,
  entry,
  onInstall,
  onInspect,
  onLifecycle,
}: {
  changeDisabled: boolean;
  disabled: boolean;
  entry: ConversationAdapterPackageCatalogEntry;
  onInstall: () => void;
  onInspect: () => void;
  onLifecycle: () => void;
}) {
  const { t } = useI18n();
  const manifestDisplayPath = manifestDisplayPathForEntry(entry);
  const packageAction = packageActionForEntry(entry);
  const lifecycleAction = packageLifecycleAction(entry);

  return (
    <article className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 rounded-xl border border-theme-card-border bg-theme-card px-3 py-2 max-[640px]:grid-cols-1">
      <div className="min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <TruncatedText
            as="h4"
            className="min-w-0 text-body-sm font-semibold text-on-surface"
          >
            {entry.item.name}
          </TruncatedText>
          <Badge tone={statusBadgeTone(entry.status)}>
            {t(statusLabelKey(entry.status))}
          </Badge>
          {entry.update_available && entry.status !== "update_available" ? (
            <Badge tone="conflict">
              {t("conversation.scriptMarket.updateAvailable")}
            </Badge>
          ) : null}
          {entry.ahead_of_release && entry.status !== "ahead_of_release" ? (
            <Badge tone="create">
              {t("conversation.scriptMarket.aheadOfRelease")}
            </Badge>
          ) : null}
        </div>
        {entry.item.description ? (
          <TruncatedText
            as="p"
            className="mt-1 text-body-sm text-on-surface-variant"
            lines={2}
          >
            {entry.item.description}
          </TruncatedText>
        ) : null}
        <div className="mt-2 flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-body-xs text-on-surface-variant">
          <span>{entry.item.provider ?? entry.item.id}</span>
          <span>{entry.item.version}</span>
          {manifestDisplayPath ? (
            <TruncatedText className="max-w-sm">
              {manifestDisplayPath}
            </TruncatedText>
          ) : null}
          {entry.item.repository_url ? (
            <a
              className="inline-flex items-center gap-1 text-primary hover:underline"
              href={entry.item.repository_url}
              rel="noreferrer"
              target="_blank"
            >
              <ExternalLink size={12} />
              {t("conversation.scriptMarket.repository")}
            </a>
          ) : null}
        </div>
        {entry.error_message ? (
          <div className="mt-2 flex items-start gap-2 rounded-xl border border-status-remove/30 bg-status-remove/10 px-2 py-1.5 text-body-xs text-status-remove">
            <CircleAlert className="mt-0.5 shrink-0" size={13} />
            <span className="min-w-0 break-words">{entry.error_message}</span>
          </div>
        ) : null}
      </div>
      <div className="flex flex-wrap items-center justify-end gap-2">
        <Button
          aria-label={t("conversation.scriptMarket.details")}
          className="inline-flex h-9 items-center gap-2 px-3 text-body-sm"
          disabled={disabled}
          onClick={onInspect}
          type="button"
          variant="outline"
        >
          <Info size={15} />
          {t("conversation.scriptMarket.details")}
        </Button>
        {lifecycleAction ? (
          <Button
            className="inline-flex h-9 items-center gap-2 px-3 text-body-sm"
            disabled={changeDisabled}
            onClick={onLifecycle}
            type="button"
            variant="outline"
          >
            <PowerOff size={15} />
            {t(actionLabelKey(lifecycleAction))}
          </Button>
        ) : null}
        {entry.installed_package?.origin === "managed_release" ? (
          <Button
            className="inline-flex h-9 items-center gap-2 px-3 text-body-sm"
            disabled={disabled}
            onClick={onInspect}
            type="button"
            variant="outline"
          >
            <Trash2 size={15} />
            {t("conversation.scriptMarket.manageVersions")}
          </Button>
        ) : null}
        {packageAction ? (
          <Button
            className="inline-flex h-9 items-center gap-2 px-3 text-body-sm"
            disabled={changeDisabled}
            onClick={onInstall}
            type="button"
          >
            <PackageActionIcon action={packageAction} />
            {t(actionLabelKey(packageAction))}
          </Button>
        ) : null}
      </div>
    </article>
  );
}

export function PackageActionIcon({
  action,
  size = 15,
}: {
  action: "install" | "update" | "repair";
  size?: number;
}) {
  if (action === "repair") {
    return <Wrench size={size} />;
  }
  if (action === "update") {
    return <ShieldCheck size={size} />;
  }
  return <Download size={size} />;
}

export function DetailField({
  label,
  value,
  wide = false,
}: {
  label: string;
  value: string;
  wide?: boolean;
}) {
  return (
    <div className={clsx("min-w-0", wide && "sm:col-span-2")}>
      <p className="text-body-xs font-medium text-on-surface-variant">
        {label}
      </p>
      <p className="mt-0.5 break-all font-mono text-body-xs text-on-surface">
        {value}
      </p>
    </div>
  );
}
