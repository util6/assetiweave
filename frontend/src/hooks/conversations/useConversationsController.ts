import { useAppSettings } from "../../store/settings/AppSettingsProvider";
import { useConversationSync } from "../../app/backgroundTasks/ConversationSyncProvider";
import { useSearchIndex } from "../../app/backgroundTasks/SearchIndexProvider";

export function useConversationsController() {
  const conversationSync = useConversationSync();
  const searchIndex = useSearchIndex();
  const { settings: appSettings } = useAppSettings();

  return {
    appSettings,
    conversationSync,
    searchIndex,
  };
}
