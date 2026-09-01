/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../i18n/I18nProvider";
import type { ConversationNavigationTarget } from "./navigationTargets";
import { AppRouter } from "./AppRouter";

const saveNavigationModelMock = vi.hoisted(() => vi.fn());

vi.mock("../hooks/catalog/useCatalogController", async () => {
  const React = await vi.importActual<typeof import("react")>("react");
  const { navigationModel } = await vi.importActual<typeof import("./menu")>("./menu");
  return {
    useCatalogController: () => {
      const [model, setModel] = React.useState({
        ...navigationModel,
        activeHeaderTabId: "memory",
        activeSubNavId: "recent",
      });
      return {
        activeTenant: { id: "tenant-1", name: "Local", slug: "local" },
        appShortcuts: [],
        applyGroupExclusiveMount: vi.fn(),
        applyAssetUpdate: vi.fn(),
        assetMountStatuses: [],
        assets: [],
        clearDeploymentPlan: vi.fn(),
        createLocalTenant: vi.fn(),
        dismissNotification: vi.fn(),
        error: null,
        expandedIds: new Set<string>(),
        loading: false,
        navigationModel: model,
        notification: null,
        profiles: [],
        refreshMountStatus: vi.fn(),
        refreshOverview: vi.fn(async () => undefined),
        refreshProfiles: vi.fn(),
        refreshingMountStatus: false,
        removeAsset: vi.fn(),
        revealPath: vi.fn(),
        saveAppShortcuts: vi.fn(),
        saveNavigationModel: async (nextModel: typeof model) => {
          saveNavigationModelMock(nextModel);
          setModel(nextModel);
          return nextModel;
        },
        setGroupMountProfile: vi.fn(),
        setMountProfiles: vi.fn(),
        showNotification: vi.fn(),
        sources: [],
        switchActiveTenant: vi.fn(),
        tenantBusy: false,
        tenants: [],
        toggleAsset: vi.fn(),
        toggleMountProfile: vi.fn(),
      };
    },
  };
});

vi.mock("../layouts/app/AppLayout", () => ({
  AppLayout: ({ children }: { children: ReactNode }) => <main>{children}</main>,
}));

vi.mock("../app/backgroundTasks/ConversationSyncProvider", () => ({
  useConversationSync: () => ({ tasks: [] }),
}));

vi.mock("../app/backgroundTasks/SearchIndexProvider", () => ({
  useSearchIndex: () => ({ task: null }),
}));

vi.mock("../app/backgroundTasks/SkillBackupProvider", () => ({
  useSkillBackup: () => ({ task: null }),
}));

vi.mock("../app/backgroundTasks/MemoryTaskProvider", () => ({
  useMemoryTasks: () => ({ tasks: [], publicTasks: [] }),
}));

vi.mock("../app/updates/AppUpdateDialog", () => ({ AppUpdateDialog: () => null }));
vi.mock("../components/backup/SkillBackupProgress", () => ({ SkillBackupBackgroundTaskIndicator: () => null }));
vi.mock("../components/conversations/ConversationToolbarControls", () => ({ ConversationBackgroundTaskIndicator: () => null }));

vi.mock("../pages/memory/MemoryPage", () => ({
  MemoryPage: ({ onNavigate }: { onNavigate?: (target: Record<string, unknown>) => void }) => (
    <button
      onClick={() =>
        onNavigate?.({
          block_id: "web-block-1",
          question_id: "web-question-1",
          record_kind: "web",
          session_id: "web-session-1",
        })
      }
      type="button"
    >
      Open web evidence
    </button>
  ),
}));

vi.mock("../pages/conversations/ConversationsPage", () => ({
  ConversationsPage: ({
    navigationTarget,
    onNavigationTargetConsumed,
    recordKind,
  }: {
    navigationTarget?: ConversationNavigationTarget | null;
    onNavigationTargetConsumed?: (nonce: string) => void;
    recordKind: "session" | "web";
  }) => (
    <div>
      <output data-testid="conversation-target">
        {navigationTarget
          ? `${recordKind}:${navigationTarget.sessionId}:${navigationTarget.questionId}:${navigationTarget.blockId}`
          : "none"}
      </output>
      {navigationTarget ? (
        <button onClick={() => onNavigationTargetConsumed?.(navigationTarget.nonce)} type="button">
          Consume target
        </button>
      ) : null}
    </div>
  ),
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("AppRouter Memory evidence navigation", () => {
  it("switches to Web Records and clears the target only after its nonce is consumed", async () => {
    render(
      <I18nProvider>
        <AppRouter />
      </I18nProvider>,
    );

    fireEvent.click(await screen.findByRole("button", { name: "Open web evidence" }));

    await waitFor(() => {
      expect(saveNavigationModelMock).toHaveBeenCalledWith(
        expect.objectContaining({ activeHeaderTabId: "conversations", activeSubNavId: "web-records" }),
      );
    });
    expect((await screen.findByTestId("conversation-target")).textContent).toBe(
      "web:web-session-1:web-question-1:web-block-1",
    );

    fireEvent.click(screen.getByRole("button", { name: "Consume target" }));
    await waitFor(() => {
      expect(screen.getByTestId("conversation-target").textContent).toBe("none");
    });
  });
});
