import { StrictMode, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import { I18nProvider, useI18n } from "../../../../i18n/I18nProvider";
import { AppUpdateProvider } from "../../../../app/updates/AppUpdateProvider";
import { AppLayout } from "../../../../layouts/app/AppLayout";
import {
  fallbackAppShortcuts,
  fallbackNavigationModel,
  fallbackProfiles,
  fallbackSources,
  fallbackAssets,
} from "../../../../mock/catalog";
import {
  AppSessionBrowser,
  ConversationShell,
  ConversationContentSearchResults,
  SessionQuestionWorkspace,
  type ConversationAppSessionGroup,
} from "../../../../pages/conversations/ConversationsPage";
import { DEFAULT_CONVERSATION_CONTENT_VISIBILITY } from "../../../conversations/ConversationContentCards";
import { DEFAULT_CONVERSATION_CONTENT_CARD_COLORS } from "../../../../store/settings/settingsSchema";
import { AssetList } from "../../../assets/AssetList";
import { RenderSafeScrollSurface } from "../RenderSafeScrollSurface";
import { createConversationRenderingStressFixture } from "./conversationRenderingStressFixture";
import { readScrollCoverage, sampleScrollCoverage } from "./scrollCoverage";
import { applyThemeToElement } from "../../../../theme/cssVars";
import type {
  ConversationSessionDetail,
  ConversationSessionListItem,
} from "../../../../types";
import "../../../../styles/index.css";

const question = createConversationRenderingStressFixture();
const session: ConversationSessionDetail = {
  session: {
    id: "rendering-stress-session",
    source_id: "fixture",
    adapter_id: "fixture",
    external_id: "fixture",
    title: "Rendering stress fixture",
    project_path: "/fixture",
    missing: false,
    created_at: "2026-08-16T00:00:00.000Z",
    imported_at: "2026-08-16T00:00:00.000Z",
  },
  questions: [question],
};
const BROWSER_SESSION_COUNT = 130;
const BROWSER_PROJECT_KEY = "/fixture/browser-project";
const noop = () => {};

function createBrowserSession(index: number): ConversationSessionListItem {
  return {
    adapter_id: "codex",
    created_at: "2026-08-16T00:00:00.000Z",
    external_id: `browser-${index + 1}`,
    id: `browser-session-${index + 1}`,
    imported_at: "2026-08-16T00:00:00.000Z",
    missing: false,
    project_path: BROWSER_PROJECT_KEY,
    question_count: 1,
    source_id: "fixture",
    title: `Browser session ${String(index + 1).padStart(3, "0")}`,
    turn_count: 1,
    updated_at: "2026-08-16T00:00:00.000Z",
  };
}

function createBrowserGroups(): ConversationAppSessionGroup[] {
  const sessions = Array.from({ length: BROWSER_SESSION_COUNT }, (_, index) =>
    createBrowserSession(index),
  );
  return [
    {
      app: { appKind: "codex", id: "codex", name: "Codex" },
      projectGroups: [
        {
          key: BROWSER_PROJECT_KEY,
          projectPath: BROWSER_PROJECT_KEY,
          sessions,
          questionCount: sessions.length,
          turnCount: sessions.length,
        },
      ],
      sessions,
      questionCount: sessions.length,
      turnCount: sessions.length,
    },
  ];
}

function DiagnosticPage() {
  const { t } = useI18n();
  const [mode, setMode] = useState("conversation");
  const [count, setCount] = useState(130);
  const [grid, setGrid] = useState(false);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [browserDetailOpen, setBrowserDetailOpen] = useState(false);
  const [browserAppId, setBrowserAppId] = useState("codex");
  const [browserProjectKey, setBrowserProjectKey] =
    useState(BROWSER_PROJECT_KEY);
  const [report, setReport] = useState("");
  const [running, setRunning] = useState(false);
  const browserGroups = useMemo(createBrowserGroups, []);
  const assets = useMemo(
    () =>
      Array.from({ length: count }, (_, index) => ({
        ...fallbackAssets[0]!,
        id: `stress-asset-${index}`,
        name: `Skill ${index + 1}`,
        discovered_at: "2026-08-16T00:00:00.000Z",
        updated_at: "2026-08-16T00:00:00.000Z",
      })),
    [count],
  );
  const surface = () => {
    const surfaces = [
      ...document.querySelectorAll<HTMLElement>(
        "[data-diagnostic-content] [data-render-safe-scroll-surface]",
      ),
    ];
    if (mode === "browser") {
      // 只有会话列挂了虚拟化集合；应用/项目列是普通列面板。
      return surfaces
        .filter(
          (element) => element.closest("[data-diagnostic-browse]") !== null,
        )
        .filter((element) =>
          element.querySelector("[data-virtualized-collection]"),
        )
        .slice(-1)[0];
    }
    return surfaces.slice(-1)[0];
  };
  async function run() {
    const target = surface();
    if (!target) return;
    setRunning(true);
    setReport("");
    let fastReadyTransitions = 0;
    const observer = new MutationObserver((records) => {
      if (target.dataset.scrollPhase !== "fast") return;
      fastReadyTransitions += records.filter(
        (record) =>
          record.type === "attributes" &&
          (record.target as HTMLElement).dataset.renderState === "ready",
      ).length;
    });
    observer.observe(target, {
      subtree: true,
      attributes: true,
      attributeFilter: ["data-render-state"],
    });
    try {
      const samples = await sampleScrollCoverage(target);
      setReport(
        JSON.stringify({
          frames: samples.length,
          fastReadyTransitions,
          failures: samples.filter((s) => s.coverage < 0.999),
          maxMounted: Math.max(...samples.map((s) => s.mountedKeys.length)),
        }),
      );
    } finally {
      observer.disconnect();
      setRunning(false);
    }
  }
  const searchResults = (
    <ConversationContentSearchResults
      contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
      includeQuestions
      loading={false}
      onCardKindToggle={noop}
      onOpenHit={noop}
      onQuestionToggle={noop}
      onSemanticRoleToggle={noop}
      onShowAllCardTypes={noop}
      selectedCardKinds={[]}
      selectedSemanticRoles={[]}
      t={t}
      result={{
        cardKinds: ["answer"],
        semanticRoles: [],
        includeQuestions: true,
        query: "fixture",
        recordKind: "session",
        totalCount: 50,
        hits: Array.from({ length: 50 }, (_, index) => ({
          block_id: `hit-${index}`,
          card_type: "answer",
          part_id: `part-${index}`,
          question_id: question.question.id,
          question_index: 0,
          question_title: `Result ${index + 1}`,
          score: 100,
          session: { ...session.session, question_count: 1, turn_count: 80 },
          snippet: `Search fixture ${index + 1}`,
          turn_id: question.turns[0]!.id,
        })),
      }}
    />
  );
  const workspace = (
    <SessionQuestionWorkspace
      session={session}
      question={question}
      questions={[question]}
      selectedQuestionId={question.question.id}
      selectedQuestionIds={new Set()}
      onQuestionSelect={noop}
      onQuestionSelectionChange={noop}
      onExport={noop}
      onPickOutputRoot={async () => null}
      outputRoot=""
      setOutputRoot={noop}
      t={t}
      visibility={DEFAULT_CONVERSATION_CONTENT_VISIBILITY}
      contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
    />
  );
  const controls = (
    <div className="flex flex-wrap gap-3 text-body-sm">
      <button onClick={() => setMode("conversation")}>Conversation</button>
      <button onClick={() => setMode("search")}>Search 50</button>
      <button
        onClick={() => {
          setMode("browser");
          setBrowserDetailOpen(false);
        }}
      >
        Session browser 130
      </button>
      {mode === "browser" ? (
        <button onClick={() => setBrowserDetailOpen((value) => !value)}>
          {browserDetailOpen ? "Back to browser" : "Open session detail"}
        </button>
      ) : null}
      <button
        onClick={() => {
          setMode("assets");
          setCount(130);
        }}
      >
        Skills 130
      </button>
      <button
        onClick={() => {
          setMode("assets");
          setCount(1000);
        }}
      >
        Skills 1000
      </button>
      <button onClick={() => setGrid((v) => !v)}>List/grid</button>
      <button
        onClick={() =>
          applyThemeToElement(document.documentElement, "sunlight")
        }
      >
        Light
      </button>
      <button
        onClick={() =>
          applyThemeToElement(document.documentElement, "midnight")
        }
      >
        Dark
      </button>
      <button
        onClick={() => {
          const e = surface();
          if (e) setReport(JSON.stringify(readScrollCoverage(e)));
        }}
      >
        Measure
      </button>
      <button
        disabled={
          running ||
          (mode === "assets" && grid) ||
          (mode === "browser" && browserDetailOpen)
        }
        onClick={() => void run()}
      >
        Run coverage
      </button>
    </div>
  );
  return (
    <AppLayout
      activeSubNavId={fallbackNavigationModel.activeSubNavId}
      appShortcuts={[]}
      navigationModel={fallbackNavigationModel}
      notification={null}
      onAppShortcutsChange={noop}
      onDismissNotification={noop}
      onHeaderTabSelect={noop}
      onNavigationModelChange={noop}
      onSubNavSelect={noop}
      tenantControls={{
        activeTenant: null,
        busy: false,
        loading: false,
        tenants: [],
        onCreateTenant: async () => {},
        onSwitchTenant: async () => {},
      }}
    >
      <div data-diagnostic-content="" className="relative min-h-0 flex-1">
        {mode !== "assets" ? (
          <ConversationShell
            title="Rendering diagnostics"
            subtitle=""
            t={t}
            onManualOpen={noop}
            headerActions={controls}
          >
            {mode === "browser" ? (
              <div className="grid min-h-0 flex-1 grid-cols-1 grid-rows-1">
                {/* 与 ConversationsPage 的保活结构一致：隐藏态只切 visibility，
                    布局盒与滚动偏移都保留，避免虚拟测量在返回后归零。 */}
                <div
                  aria-hidden={browserDetailOpen}
                  className="col-start-1 row-start-1 flex min-h-0 min-w-0 flex-col"
                  data-diagnostic-browse=""
                  inert={browserDetailOpen}
                  style={
                    browserDetailOpen ? { visibility: "hidden" } : undefined
                  }
                >
                  <AppSessionBrowser
                    active={!browserDetailOpen}
                    appShortcuts={fallbackAppShortcuts}
                    groups={browserGroups}
                    onAppSelect={setBrowserAppId}
                    onProjectSelect={setBrowserProjectKey}
                    onSessionOpen={() => setBrowserDetailOpen(true)}
                    selectedAppId={browserAppId}
                    selectedProjectKey={browserProjectKey}
                    t={t}
                  />
                </div>
                {browserDetailOpen ? (
                  <div className="col-start-1 row-start-1 flex min-h-0 min-w-0 flex-col">
                    {workspace}
                  </div>
                ) : null}
              </div>
            ) : null}
            {mode === "search" ? searchResults : null}
            {mode !== "browser" ? workspace : null}
          </ConversationShell>
        ) : (
          <section className="app-bounded-route flex flex-col gap-4 p-6">
            <header className="shrink-0">{controls}</header>
            <RenderSafeScrollSurface className="min-h-0 flex-1" tabIndex={0}>
              <AssetList
                assets={assets}
                sources={fallbackSources}
                profiles={fallbackProfiles}
                appShortcuts={fallbackAppShortcuts}
                assetMountStatuses={[]}
                expandedIds={expanded}
                viewMode={grid ? "grid" : "list"}
                onToggleAsset={(id) =>
                  setExpanded((previous) => {
                    const next = new Set(previous);
                    if (next.has(id)) next.delete(id);
                    else next.add(id);
                    return next;
                  })
                }
                onToggleMount={noop}
                onRevealPath={noop}
                onEditAsset={noop}
                onDeleteAsset={noop}
              />
            </RenderSafeScrollSurface>
          </section>
        )}
        <output
          data-testid="rendering-report"
          className="fixed bottom-0 left-0 z-50 max-h-24 w-full overflow-auto bg-background text-code-sm"
        >
          {report}
        </output>
      </div>
    </AppLayout>
  );
}
// Separate Vite HTML entry, not linked by production navigation or imported by main.
if (import.meta.env.DEV) {
  const root = createRoot(document.getElementById("root")!);
  root.render(
    <StrictMode>
      <I18nProvider>
        <AppUpdateProvider>
          <DiagnosticPage />
        </AppUpdateProvider>
      </I18nProvider>
    </StrictMode>,
  );
  import.meta.hot?.dispose(() => root.unmount());
}
