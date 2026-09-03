import {
  queryOptions,
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from "@tanstack/react-query";
import {
  getAppSettings,
  saveAppSettings,
  type AppSettingsFile,
} from "../../services/appSettings";
import {
  defaultSettings,
  defaultStorageInfo,
  normalizeStoredSettings,
  type AppSettings,
} from "./settingsSchema";
import { readCachedSettings, writeCachedSettings } from "./settingsPersistence";

export const appSettingsKey = ["app-settings"] as const;
export const saveAppSettingsMutationKey = ["app-settings", "save"] as const;

export function settingsQueryOptions() {
  return queryOptions<AppSettingsFile>({
    queryKey: appSettingsKey,
    queryFn: async () => {
      const file = await getAppSettings();
      const normalized = normalizeStoredSettings(file.settings);
      writeCachedSettings(normalized);
      return file;
    },
    initialData: () => {
      const cached = readCachedSettings();
      return {
        config_dir: defaultStorageInfo.configDir,
        config_path: defaultStorageInfo.configPath,
        conversation_adapter_dir: defaultStorageInfo.conversationAdapterDir,
        settings: cached,
      };
    },
    initialDataUpdatedAt: 0,
  });
}

export function useSaveAppSettings(): UseMutationResult<
  AppSettingsFile,
  unknown,
  AppSettings
> {
  const queryClient = useQueryClient();

  return useMutation({
    mutationKey: saveAppSettingsMutationKey,
    scope: { id: "app-settings" },
    mutationFn: async (settings: AppSettings) => {
      return await saveAppSettings(settings);
    },
    onSuccess: (data, _variables, _context) => {
      const normalized = normalizeStoredSettings(data.settings);
      writeCachedSettings(normalized);
      queryClient.setQueryData(appSettingsKey, data);

      const mutationCache = queryClient.getMutationCache();
      const saveMutations = mutationCache.getAll().filter((m) => {
        const key = m.options.mutationKey;
        return (
          Array.isArray(key) && key[0] === "app-settings" && key[1] === "save"
        );
      });

      // Remove settled mutations older than or equal to this one to prevent old errors resurrecting
      for (const m of saveMutations) {
        if (m.state.status !== "pending") {
          mutationCache.remove(m);
        }
      }
    },
  });
}
