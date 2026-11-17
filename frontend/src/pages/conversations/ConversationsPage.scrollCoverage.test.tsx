/* @vitest-environment jsdom */

import {
  act,
  cleanup,
  configure,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { defaultSettings } from "../../store/settings/settingsSchema";
import type {
  ConversationAdapter,
  ConversationSessionDetail,
  ConversationSessionListItem,
} from "../../types";
import {
  readScrollCoverage,
  sampleScrollCoverage,
} from "../../components/common/rendering/__fixtures__/scrollCoverage";
import { ConversationsPage } from "./ConversationsPage";

const getConversationSessionMock = vi.hoisted(() => vi.fn());
const listConversationAdaptersMock = vi.hoisted(() => vi.fn());
const listConversationSessionsMock = vi.hoisted(() => vi.fn());

vi.mock("../../app/backgroundTasks/ConversationSyncProvider", () => ({
  useConversationSync: () => ({ startSync: vi.fn(), taskFor: () => null }),
}));

vi.mock("../../app/backgroundTasks/SearchIndexProvider", () => ({
  useSearchIndex: () => ({
    rebuild: vi.fn(),
    refresh: vi.fn(),
    status: { health: "ready" },
    task: null,
  }),
}));

vi.mock("../../store/settings/useAppSettings", async () => {
  const actual = await vi.importActual<
    typeof import("../../store/settings/useAppSettings")
  >("../../store/settings/useAppSettings");
  return {
    ...actual,
    useAppSettings: () => ({
      resetSettings: vi.fn(),
      retrySave: vi.fn(),
      setColumnLayout: vi.fn(),
      setColumnLayoutAsync: vi.fn().mockResolvedValue(undefined),
      settings: defaultSettings,
      settingsError: null,
      settingsLoaded: true,
      storageInfo: {},
      updateSetting: vi.fn(),
    }),
  };
});

vi.mock("../../services/conversations", async () => {
  const actual = await vi.importActual<
    typeof import("../../services/conversations")
  >("../../services/conversations");
  return {
    ...actual,
    getConversationSession: getConversationSessionMock,
    listConversationAdapters: listConversationAdaptersMock,
    listConversationSessions: listConversationSessionsMock,
  };
});

const now = "2026-11-09T00:00:00Z";
const SESSION_COUNT = 40;
const VIEWPORT_HEIGHT = 640;
const VIEWPORT_WIDTH = 1200;
const ROW_HEIGHT = 132;
const ROW_GAP = 12;
const ROW_PITCH = ROW_HEIGHT + ROW_GAP;

// 整页 + 虚拟化 + 渲染调度器在并行跑多个测试文件时可能超出默认 1s。
configure({ asyncUtilTimeout: 10_000 });

let testQueryClient: QueryClient;

describe("ConversationsPage browse-list scroll coverage", () => {
  beforeEach(() => {
    testQueryClient = new QueryClient({
      defaultOptions: {
        queries: { gcTime: 0, retry: false, staleTime: 60_000 },
      },
    });
    localStorage.removeItem("assetiweave.locale");
    window.scrollTo = vi.fn();
    stubLayout();
    listConversationAdaptersMock.mockReset().mockResolvedValue([adapter]);
    listConversationSessionsMock
      .mockReset()
      .mockResolvedValue(sessionListItems());
    getConversationSessionMock
      .mockReset()
      .mockImplementation(async (sessionId: string) =>
        sessionDetail(sessionId),
      );
  });

  afterEach(() => {
    cleanup();
    testQueryClient.clear();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    vi.clearAllMocks();
  });

  it("keeps a fully covered bounded viewport on the browse list across the detail round trip", async () => {
    renderPage();

    await screen.findByRole("button", { name: "Open session Alpha 01" });
    const listSurface = browseSurface();

    // 进入详情前：有界视口 + 首屏已被虚拟行完整覆盖。
    const initial = readScrollCoverage(listSurface);
    expectCoveredViewport(initial, "before opening the session");

    act(() => {
      listSurface.scrollTop = 400;
      fireEvent.scroll(listSurface, { target: { scrollTop: 400 } });
    });
    const cardAtOffset = await screen.findByRole("button", {
      name: "Open session Alpha 06",
    });
    expectCoveredViewport(
      readScrollCoverage(listSurface),
      "before opening the session at offset 400",
    );

    fireEvent.click(cardAtOffset);
    await screen.findByRole("button", { name: "Back to apps / sessions" });

    fireEvent.click(
      screen.getByRole("button", { name: "Back to apps / sessions" }),
    );

    // 返回后的同一提交里就必须有行覆盖：保活视图不允许出现“先空白、再补救”的帧。
    const returnedSurface = browseSurface();
    const noBlankFrame = readScrollCoverage(returnedSurface);
    expectCoveredViewport(noBlankFrame, "immediately after returning");
    expect(noBlankFrame.scrollTop).toBe(400);
    expect(returnedSurface).toBe(listSurface);

    // 返回后逐个跳转采样：任何一处视口都必须被已挂载的虚拟行完整覆盖。
    const samples = await act(async () =>
      sampleScrollCoverage(returnedSurface),
    );
    expect(samples).toHaveLength(48);
    for (const sample of samples) {
      expectCoveredViewport(sample, `after returning at ratio ${sample.ratio}`);
    }
    expect(readScrollCoverage(returnedSurface).scrollTop).toBe(400);
  }, 30_000);
});

/** 只在“进入详情前”与“返回后”都成立的几何与覆盖不变量。 */
function expectCoveredViewport(
  coverage: ReturnType<typeof readScrollCoverage>,
  label: string,
) {
  expect(coverage.viewportValid, `${label}: viewport geometry`).toBe(true);
  expect(coverage.clientHeight, `${label}: bounded viewport`).toBe(
    VIEWPORT_HEIGHT,
  );
  expect(
    coverage.scrollHeight,
    `${label}: content is longer than the viewport`,
  ).toBeGreaterThan(coverage.clientHeight);
  expect(
    coverage.mountedKeys.length,
    `${label}: mounted virtual rows`,
  ).toBeGreaterThan(0);
  expect(
    coverage.coverage,
    `${label}: viewport coverage`,
  ).toBeGreaterThanOrEqual(0.999);
}

/** 会话列表是唯一同时挂着虚拟化集合的滚动面。 */
function browseSurface() {
  const surfaces = [
    ...document.querySelectorAll<HTMLElement>(
      "[data-render-safe-scroll-surface]",
    ),
  ].filter((surface) => surface.querySelector("[data-virtualized-collection]"));
  expect(surfaces).toHaveLength(1);
  return surfaces[0]!;
}

function renderPage() {
  return render(
    <QueryClientProvider client={testQueryClient}>
      <I18nProvider>
        <ConversationsPage
          appShortcuts={[]}
          onManualOpen={vi.fn()}
          onNotify={vi.fn()}
          onNotifyError={vi.fn()}
          onOpenSettings={vi.fn()}
          recordKind="session"
        />
      </I18nProvider>
    </QueryClientProvider>,
  );
}

function rect(top: number, height: number): DOMRect {
  return {
    bottom: top + height,
    height,
    left: 0,
    right: VIEWPORT_WIDTH,
    top,
    width: VIEWPORT_WIDTH,
    x: 0,
    y: top,
    toJSON: () => ({}),
  } as DOMRect;
}

/**
 * jsdom 没有布局：视口、滚动高度与每一行的位置全部为 0。
 * 这里给出与真实虚拟列表一致的最小几何模型：虚拟行按 `data-index` 落在
 * 绝对定位轨道上，并随滚动容器偏移；集合高度取 react-virtual 的 total size。
 */
function stubLayout() {
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    // 只有滚动面拥有有界视口；其它元素为 0，让 viewportValid 证明的是布局而不是桩。
    get(this: HTMLElement) {
      return this.hasAttribute("data-render-safe-scroll-surface")
        ? VIEWPORT_HEIGHT
        : 0;
    },
  });
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get: () => VIEWPORT_WIDTH,
  });
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
    configurable: true,
    get: () => ROW_HEIGHT,
  });
  Object.defineProperty(HTMLElement.prototype, "scrollHeight", {
    configurable: true,
    get(this: HTMLElement) {
      if (!this.hasAttribute("data-render-safe-scroll-surface")) {
        return this.clientHeight;
      }
      const collection = this.querySelector<HTMLElement>(
        "[data-virtualized-collection]",
      );
      return Math.max(
        this.clientHeight,
        Number.parseFloat(collection?.style.height ?? "0") || 0,
      );
    },
  });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    function (this: HTMLElement) {
      if (this.hasAttribute("data-render-safe-scroll-surface")) {
        return rect(0, VIEWPORT_HEIGHT);
      }
      if (this.hasAttribute("data-virtualized-collection")) {
        return rect(0, Number.parseFloat(this.style.height) || 0);
      }
      if (this.dataset.virtualItemKey) {
        const surface = this.closest<HTMLElement>(
          "[data-render-safe-scroll-surface]",
        );
        const index = Number.parseInt(this.dataset.index ?? "0", 10);
        return rect(index * ROW_PITCH - (surface?.scrollTop ?? 0), ROW_HEIGHT);
      }
      return rect(0, 0);
    },
  );
  vi.stubGlobal(
    "ResizeObserver",
    class {
      private readonly callback: ResizeObserverCallback;
      constructor(callback: ResizeObserverCallback) {
        this.callback = callback;
      }
      observe(target: Element) {
        if (!target.hasAttribute("data-render-safe-scroll-surface")) return;
        const element = target as HTMLElement;
        queueMicrotask(() =>
          this.callback(
            [
              {
                target,
                borderBoxSize: [
                  {
                    blockSize: element.clientHeight,
                    inlineSize: element.clientWidth,
                  },
                ],
                contentRect: {
                  height: element.clientHeight,
                  width: element.clientWidth,
                },
              } as unknown as ResizeObserverEntry,
            ],
            this as unknown as ResizeObserver,
          ),
        );
      }
      unobserve() {}
      disconnect() {}
    },
  );
}

const adapter: ConversationAdapter = {
  capabilities: ["probe", "read_session"],
  content_hash: "adapter-hash",
  created_at: now,
  enabled: true,
  executable_path: "/tmp/codex-adapter",
  id: "codex",
  input_kinds: ["directory"],
  kind: "external",
  manifest_path: "/tmp/codex-adapter.json",
  name: "Codex",
  protocol_version: 1,
  trust_state: "trusted",
  trusted_hash: "adapter-hash",
  updated_at: now,
  version: "0.1.0",
};

function sessionListItems(): ConversationSessionListItem[] {
  return Array.from({ length: SESSION_COUNT }, (_, index) => ({
    adapter_id: "codex",
    created_at: now,
    external_id: `alpha-${index + 1}`,
    id: `alpha-session-${index + 1}`,
    imported_at: now,
    missing: false,
    project_path: "/repo/alpha",
    question_count: 1,
    source_id: "codex-live",
    title: `Alpha ${String(index + 1).padStart(2, "0")}`,
    turn_count: 1,
    updated_at: now,
  }));
}

function sessionDetail(sessionId: string): ConversationSessionDetail {
  const session = sessionListItems().find((item) => item.id === sessionId);
  return {
    questions: [
      {
        parts: [],
        projected_content_nodes: [],
        question: {
          created_at: now,
          id: `question-${sessionId}`,
          session_id: sessionId,
          title: `Question of ${sessionId}`,
          updated_at: now,
        },
        question_turns: [],
        turns: [],
      },
    ],
    session: session ?? sessionListItems()[0]!,
  };
}
