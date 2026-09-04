import { useContext } from "react";
import {
  QueryClient,
  QueryClientContext,
  useMutationState,
  useQuery,
  type QueryClient as QueryClientType,
} from "@tanstack/react-query";
import {
  appSettingsKey,
  saveAppSettingsMutationKey,
  settingsQueryOptions,
  useSaveAppSettings,
} from "./settingsQueries";
import {
  defaultSettings,
  defaultStorageInfo,
  normalizeStoredSettings,
  type AppSettings,
  type AppSettingsStorageInfo,
} from "./settingsSchema";
import type { AppSettingsFile } from "../../services/appSettings";

const fallbackQueryClient = new QueryClient({
  defaultOptions: {
    queries: { enabled: false, retry: false },
    mutations: { retry: false },
  },
});

export interface AppSettingsContextValue {
  resetSettings: () => void;
  retrySave: () => void;
  setColumnLayout: (storageKey: string, weights: number[]) => void;
  setColumnLayoutAsync: (
    storageKey: string,
    weights: number[],
  ) => Promise<void>;
  settings: AppSettings;
  settingsError: string | null;
  settingsLoaded: boolean;
  storageInfo: AppSettingsStorageInfo;
  updateSetting: <Key extends keyof AppSettings>(
    key: Key,
    value: AppSettings[Key],
  ) => void;
}

export function getCurrentProjectedSettings(
  queryClient: QueryClientType,
): AppSettings {
  const mutationCache = queryClient.getMutationCache();
  const saveMutations = mutationCache
    .getAll()
    .filter((m) => {
      const key = m.options.mutationKey;
      return (
        Array.isArray(key) && key[0] === "app-settings" && key[1] === "save"
      );
    })
    .sort((a, b) => a.mutationId - b.mutationId);

  const latestMutation = saveMutations[saveMutations.length - 1];
  if (
    latestMutation &&
    (latestMutation.state.status === "pending" ||
      latestMutation.state.status === "error") &&
    latestMutation.state.variables
  ) {
    return latestMutation.state.variables as AppSettings;
  }

  const queryData = queryClient.getQueryData<AppSettingsFile>(appSettingsKey);
  return normalizeStoredSettings(queryData?.settings);
}

export function useAppSettings(): AppSettingsContextValue {
  const contextClient = useContext(QueryClientContext);
  const queryClient = contextClient ?? fallbackQueryClient;
  const isEnabled = Boolean(contextClient);

  const query = useQuery(
    {
      ...settingsQueryOptions(),
      enabled: isEnabled,
    },
    queryClient,
  );
  const saveMutation = useSaveAppSettings(queryClient);

  const mutationStates = useMutationState(
    {
      filters: { mutationKey: saveAppSettingsMutationKey },
      select: (mutation) => ({
        error: mutation.state.error,
        id: mutation.mutationId,
        status: mutation.state.status,
        variables: mutation.state.variables as AppSettings | undefined,
      }),
    },
    queryClient,
  );

  const sortedMutations = [...mutationStates].sort((a, b) => a.id - b.id);
  const latestMutation = sortedMutations[sortedMutations.length - 1];

  const confirmedSettings = normalizeStoredSettings(query.data?.settings);
  const settings: AppSettings =
    latestMutation &&
    (latestMutation.status === "pending" ||
      latestMutation.status === "error") &&
    latestMutation.variables
      ? latestMutation.variables
      : confirmedSettings;

  const settingsError =
    latestMutation && latestMutation.status === "error"
      ? errorMessage(latestMutation.error)
      : query.error
        ? errorMessage(query.error)
        : null;

  const settingsLoaded = !query.isLoading;

  const storageInfo: AppSettingsStorageInfo = {
    ...defaultStorageInfo,
    configDir:
      query.data?.display_config_dir ??
      query.data?.config_dir ??
      defaultStorageInfo.configDir,
    configPath:
      query.data?.display_config_path ??
      query.data?.config_path ??
      defaultStorageInfo.configPath,
    conversationAdapterDir:
      query.data?.display_conversation_adapter_dir ??
      query.data?.conversation_adapter_dir ??
      defaultStorageInfo.conversationAdapterDir,
  };

  const updateSetting = <Key extends keyof AppSettings>(
    key: Key,
    value: AppSettings[Key],
  ) => {
    const current = getCurrentProjectedSettings(queryClient);
    const nextSettings: AppSettings = {
      ...current,
      [key]: value,
    };
    saveMutation.mutate(nextSettings);
  };

  const resetSettings = () => {
    const current = getCurrentProjectedSettings(queryClient);
    const nextSettings: AppSettings = {
      ...defaultSettings,
      locale: current.locale,
    };
    saveMutation.mutate(nextSettings);
  };

  const setColumnLayoutAsync = async (
    storageKey: string,
    weights: number[],
  ): Promise<void> => {
    const current = getCurrentProjectedSettings(queryClient);
    const nextSettings: AppSettings = {
      ...current,
      columnLayouts: {
        ...current.columnLayouts,
        [storageKey]: weights,
      },
    };
    await saveMutation.mutateAsync(nextSettings);
  };

  const setColumnLayout = (storageKey: string, weights: number[]) => {
    void setColumnLayoutAsync(storageKey, weights).catch(() => undefined);
  };

  const retrySave = () => {
    if (
      latestMutation &&
      latestMutation.status === "error" &&
      latestMutation.variables
    ) {
      saveMutation.mutate(latestMutation.variables);
    }
  };

  return {
    resetSettings,
    retrySave,
    setColumnLayout,
    setColumnLayoutAsync,
    settings,
    settingsError,
    settingsLoaded,
    storageInfo,
    updateSetting,
  };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
