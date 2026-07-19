import { PackageCheck } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import type { ConversationRecordKind } from "../../types";
import { DialogFrame } from "../foundation/DialogFrame";
import type { NotificationMessage } from "../notifications/NotificationBanner";
import { Button } from "../ui/button";
import { ConversationScriptResourcePanel } from "./ConversationScriptResourcePanel";

type ConversationImportNotification = Omit<NotificationMessage, "id">;

export function ConversationImportDialog({
  busy = false,
  onClose,
  onNotify = () => undefined,
  onNotifyError = () => undefined,
  onScriptInstalled = () => undefined,
  recordKind,
}: {
  busy?: boolean;
  onClose: () => void;
  onNotify?: (notification: ConversationImportNotification) => void;
  onNotifyError?: (message: string) => void;
  onScriptInstalled?: () => Promise<void> | void;
  recordKind: ConversationRecordKind;
}) {
  const { t } = useI18n();

  return (
    <DialogFrame
      busy={busy}
      closeLabel={t("conversation.import.close")}
      icon={<PackageCheck size={19} />}
      onClose={onClose}
      size="xl"
      title={t("conversation.scriptMarket.inlineTitle")}
      footer={
        <Button disabled={busy} onClick={onClose} type="button" variant="ghost">
          {t("common.cancel")}
        </Button>
      }
    >
      <ConversationScriptResourcePanel
        disabled={busy}
        onInstalled={onScriptInstalled}
        onNotify={onNotify}
        onNotifyError={onNotifyError}
        recordKind={recordKind}
      />
    </DialogFrame>
  );
}
