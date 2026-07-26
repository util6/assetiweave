/* @vitest-environment jsdom */

import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type {
  MemoryItem,
  MemoryItemDetail,
  MemoryItemListParams,
  MemoryItemPageResult,
} from "../../types/memory";
import { MemoryPage } from "./MemoryPage";

const memoryService = vi.hoisted(() => ({
  acceptMemoryCandidate: vi.fn(),
  archiveMemoryItem: vi.fn(),
  archiveMemoryDreamNote: vi.fn(),
  createMemoryItem: vi.fn(),
  getMemoryDreamNote: vi.fn(),
  getMemoryOverview: vi.fn(),
  getMemoryItem: vi.fn(),
  listMemoryDreamNotes: vi.fn(),
  listMemoryItems: vi.fn(),
  previewMemoryDream: vi.fn(),
  previewMemoryRecall: vi.fn(),
  promoteMemoryDreamNote: vi.fn(),
  rejectMemoryCandidate: vi.fn(),
  updateMemoryItem: vi.fn(),
}));

const memoryTasks = vi.hoisted(() => ({
  autoDreamStatus: null as ReturnType<typeof createDreamPreview> | null,
  cancelTask: vi.fn(),
  refreshAutoDreamStatus: vi.fn(),
  startDream: vi.fn(),
  startRecall: vi.fn(),
  task: null,
  tasks: [],
}));

vi.mock("../../services/memory", () => memoryService);
vi.mock("../../app/backgroundTasks/MemoryTaskProvider", () => ({
  useMemoryTasks: () => memoryTasks,
}));

beforeEach(() => {
  vi.stubGlobal("localStorage", createMockLocalStorage());
  localStorage.setItem("assetiweave.locale", "zh");
  vi.clearAllMocks();
  memoryService.getMemoryOverview.mockResolvedValue(null);
  memoryService.listMemoryDreamNotes.mockResolvedValue(null);
  memoryTasks.refreshAutoDreamStatus.mockResolvedValue(undefined);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("MemoryPage", () => {
  it("loads Overview as a deterministic local aggregation", async () => {
    memoryService.getMemoryOverview.mockResolvedValue({
      candidate_count: 2,
      dream_status: createDreamPreview(),
      follow_ups: [],
      latest_dream: null,
      recent_items: [],
      stale_count: 1,
    });

    renderMemoryPage("overview");

    expect(screen.getByRole("heading", { level: 1, name: "今日 / 继续工作" })).toBeTruthy();
    expect(await screen.findByText("2")).toBeTruthy();
    expect(memoryService.getMemoryOverview).toHaveBeenCalledTimes(1);
    expect(memoryService.previewMemoryDream).not.toHaveBeenCalled();
    expect(memoryService.listMemoryItems).not.toHaveBeenCalled();
  });

  it("shows Dream gates and the locally selected delta", async () => {
    memoryService.listMemoryDreamNotes.mockResolvedValue({ items: [], limit: 100, offset: 0, total_count: 0 });
    memoryTasks.autoDreamStatus = createDreamPreview();

    renderMemoryPage("dreams");

    expect(screen.getByRole("heading", { level: 1, name: "自动 Dream" })).toBeTruthy();
    expect(await screen.findByText("时间间隔")).toBeTruthy();
    expect(screen.getByText("3/3")).toBeTruthy();
    expect(memoryService.listMemoryDreamNotes).toHaveBeenCalledTimes(1);
    expect(memoryService.listMemoryItems).not.toHaveBeenCalled();
  });

  it("offers evidence-only Recall without calling AI on open", () => {
    renderMemoryPage("recall");

    expect(screen.getByRole("heading", { level: 1, name: "深度回忆" })).toBeTruthy();
    expect(screen.getByText("尚未构建证据包")).toBeTruthy();
    expect(memoryService.previewMemoryRecall).not.toHaveBeenCalled();
    expect(memoryTasks.startRecall).not.toHaveBeenCalled();
    expect(memoryService.listMemoryItems).not.toHaveBeenCalled();
  });

  it("previews Recall coverage and opens exact evidence without AI", async () => {
    const onEvidenceOpen = vi.fn();
    memoryService.previewMemoryRecall.mockResolvedValue(createRecallPreview());
    renderMemoryPage("recall", onEvidenceOpen);

    fireEvent.change(screen.getByRole("textbox", { name: "回忆问题" }), { target: { value: "为什么使用 AppService？" } });
    fireEvent.click(screen.getByRole("button", { name: "构建本地证据包" }));

    expect(await screen.findByText("Use AppService boundary")).toBeTruthy();
    expect(screen.getByText(/检索后端.*tantivy/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /evidence-0/ }));
    expect(onEvidenceOpen).toHaveBeenCalledWith(expect.objectContaining({ block_id: "block-1", question_id: "question-1" }));
    expect(memoryTasks.startRecall).not.toHaveBeenCalled();
  });

  it("shows loading and an explicit browser-preview empty state", async () => {
    let resolvePage: ((page: MemoryItemPageResult) => void) | undefined;
    memoryService.listMemoryItems.mockReturnValueOnce(
      new Promise<MemoryItemPageResult>((resolve) => {
        resolvePage = resolve;
      }),
    );

    renderMemoryPage();

    expect(screen.getByText("正在加载 Memory 库…")).toBeTruthy();

    await act(async () => {
      resolvePage?.({
        availability: "browser_preview",
        items: [],
        limit: 50,
        offset: 0,
        total_count: 0,
      });
    });

    expect(await screen.findByText("浏览器预览不提供 Memory 数据")).toBeTruthy();
    expect((screen.getByRole("button", { name: "新建 Memory" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("shows a recoverable list error", async () => {
    memoryService.listMemoryItems
      .mockRejectedValueOnce(new Error("database unavailable"))
      .mockResolvedValueOnce(createPage([createItem("memory-1", "Recovered decision")]));
    memoryService.getMemoryItem.mockResolvedValue(createDetail(createItem("memory-1", "Recovered decision")));

    renderMemoryPage();

    expect((await screen.findByRole("alert")).textContent).toContain("database unavailable");
    fireEvent.click(screen.getByRole("button", { name: "重试" }));

    expect(await screen.findByText("Recovered decision")).toBeTruthy();
    expect(memoryService.listMemoryItems).toHaveBeenCalledTimes(2);
  });

  it("browses, searches, filters, paginates, and exposes evidence plus revisions", async () => {
    const decision = createItem("memory-1", "Use AppService boundary", {
      kind: "decision",
      stale_reason: "evidence_changed",
    });
    const method = createItem("memory-2", "Batch refresh once", { kind: "method" });
    memoryService.listMemoryItems.mockImplementation(async (params: MemoryItemListParams = {}) => ({
      ...createPage(params.offset ? [method] : [decision, method], 80),
      offset: params.offset ?? 0,
    }));
    memoryService.getMemoryItem.mockImplementation(async (itemId: string) =>
      createDetail(itemId === decision.id ? decision : method),
    );

    renderMemoryPage();

    expect(await screen.findByText(decision.title)).toBeTruthy();
    expect(await screen.findByText("来自原始对话的证据摘录")).toBeTruthy();
    expect(screen.getByText("版本 1 · 创建")).toBeTruthy();

    const search = screen.getByRole("searchbox", { name: "搜索当前页 Memory" });
    fireEvent.change(search, { target: { value: "batch" } });
    fireEvent.keyDown(search, { key: "Enter" });
    expect(await screen.findByText(method.title)).toBeTruthy();
    expect(screen.queryByText(decision.title)).toBeNull();

    fireEvent.pointerDown(screen.getByRole("button", { name: "按类型筛选" }), { button: 0, ctrlKey: false });
    fireEvent.click(await screen.findByRole("menuitemcheckbox", { name: "决定" }));
    await waitFor(() => {
      expect(memoryService.listMemoryItems).toHaveBeenLastCalledWith(
        expect.objectContaining({ kinds: ["decision"], limit: 50, offset: 0 }),
      );
    });
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });

    fireEvent.click(await screen.findByRole("button", { name: "下一页" }));
    await waitFor(() => {
      expect(memoryService.listMemoryItems).toHaveBeenLastCalledWith(
        expect.objectContaining({ limit: 50, offset: 50 }),
      );
    });
  });

  it("sends status, origin, and freshness filters to the service", async () => {
    const item = createItem("memory-1", "Filter target");
    memoryService.listMemoryItems.mockResolvedValue(createPage([item]));
    memoryService.getMemoryItem.mockResolvedValue(createDetail(item));

    renderMemoryPage();
    expect(await screen.findByRole("button", { name: item.title })).toBeTruthy();

    fireEvent.pointerDown(screen.getByRole("button", { name: "按状态筛选" }), { button: 0, ctrlKey: false });
    fireEvent.click(await screen.findByRole("menuitemcheckbox", { name: "候选" }));
    await waitFor(() => {
      expect(memoryService.listMemoryItems).toHaveBeenLastCalledWith(expect.objectContaining({ statuses: ["candidate"] }));
    });
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });

    fireEvent.pointerDown(await screen.findByRole("button", { name: "按来源筛选" }), { button: 0, ctrlKey: false });
    fireEvent.click(await screen.findByRole("menuitemcheckbox", { name: "手工" }));
    await waitFor(() => {
      expect(memoryService.listMemoryItems).toHaveBeenLastCalledWith(
        expect.objectContaining({ origins: ["manual"], statuses: ["candidate"] }),
      );
    });
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });

    fireEvent.pointerDown(await screen.findByRole("button", { name: "按新鲜度筛选" }), { button: 0, ctrlKey: false });
    fireEvent.click(await screen.findByRole("menuitem", { name: "仅证据异常" }));
    await waitFor(() => {
      expect(memoryService.listMemoryItems).toHaveBeenLastCalledWith(expect.objectContaining({ stale_only: true }));
    });
  });

  it("creates, edits, and archives a formal Memory item", async () => {
    const active = createItem("memory-1", "Existing decision");
    memoryService.listMemoryItems.mockResolvedValue(createPage([active]));
    memoryService.getMemoryItem.mockResolvedValue(createDetail(active));
    memoryService.createMemoryItem.mockResolvedValue(createDetail(createItem("memory-new", "New method", { kind: "method" })));
    memoryService.updateMemoryItem.mockResolvedValue(createDetail({ ...active, title: "Edited decision" }));
    memoryService.archiveMemoryItem.mockResolvedValue(createDetail({ ...active, status: "archived" }));

    renderMemoryPage();
    expect(await screen.findByText(active.title)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "新建 Memory" }));
    const createDialog = screen.getByRole("dialog", { name: "新建 Memory" });
    fireEvent.change(within(createDialog).getByLabelText("类型"), { target: { value: "method" } });
    fireEvent.change(within(createDialog).getByLabelText("标题"), { target: { value: "New method" } });
    fireEvent.change(within(createDialog).getByLabelText("内容"), { target: { value: "Run one catalog refresh after a batch." } });
    fireEvent.click(within(createDialog).getByRole("button", { name: "创建" }));
    await waitFor(() => {
      expect(memoryService.createMemoryItem).toHaveBeenCalledWith(
        expect.objectContaining({
          content_markdown: "Run one catalog refresh after a batch.",
          kind: "method",
          title: "New method",
        }),
      );
    });

    fireEvent.click(await screen.findByRole("button", { name: active.title }));
    expect(await screen.findByRole("button", { name: "编辑 Memory" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "编辑 Memory" }));
    const editDialog = screen.getByRole("dialog", { name: "编辑 Memory" });
    fireEvent.change(within(editDialog).getByLabelText("标题"), { target: { value: "Edited decision" } });
    fireEvent.click(within(editDialog).getByRole("button", { name: "保存" }));
    await waitFor(() => {
      expect(memoryService.updateMemoryItem).toHaveBeenCalledWith(
        expect.objectContaining({ item_id: active.id, title: "Edited decision" }),
      );
    });

    fireEvent.click(await screen.findByRole("button", { name: "归档 Memory" }));
    const confirmDialog = screen.getByRole("dialog", { name: "归档 Memory" });
    fireEvent.click(within(confirmDialog).getByRole("button", { name: "确认归档" }));
    await waitFor(() => {
      expect(memoryService.archiveMemoryItem).toHaveBeenCalledWith(active.id);
    });
  });

  it("lets candidates be edited before acceptance or explicitly rejected", async () => {
    const candidateA = createItem("candidate-a", "Candidate A", { origin: "deep_recall", status: "candidate" });
    const candidateB = createItem("candidate-b", "Candidate B", { origin: "auto_dream", status: "candidate" });
    memoryService.listMemoryItems.mockResolvedValue(createPage([candidateA, candidateB]));
    memoryService.getMemoryItem.mockImplementation(async (itemId: string) =>
      createDetail(itemId === candidateA.id ? candidateA : candidateB),
    );
    memoryService.acceptMemoryCandidate.mockResolvedValue(createDetail({ ...candidateA, status: "active", title: "Accepted candidate" }));
    memoryService.rejectMemoryCandidate.mockResolvedValue(createDetail({ ...candidateB, status: "rejected" }));

    renderMemoryPage();
    expect(await screen.findByText(candidateA.title)).toBeTruthy();

    fireEvent.click(await screen.findByRole("button", { name: "编辑后接受" }));
    const acceptDialog = screen.getByRole("dialog", { name: "编辑并接受候选" });
    fireEvent.change(within(acceptDialog).getByLabelText("标题"), { target: { value: "Accepted candidate" } });
    fireEvent.click(within(acceptDialog).getByRole("button", { name: "接受候选" }));
    await waitFor(() => {
      expect(memoryService.acceptMemoryCandidate).toHaveBeenCalledWith(
        expect.objectContaining({ item_id: candidateA.id, title: "Accepted candidate" }),
      );
    });

    fireEvent.click(await screen.findByRole("button", { name: candidateB.title }));
    await waitFor(() => {
      expect(memoryService.getMemoryItem).toHaveBeenLastCalledWith(candidateB.id);
    });
    fireEvent.click(screen.getByRole("button", { name: "拒绝候选" }));
    const rejectDialog = screen.getByRole("dialog", { name: "拒绝候选" });
    fireEvent.click(within(rejectDialog).getByRole("button", { name: "确认拒绝" }));
    await waitFor(() => {
      expect(memoryService.rejectMemoryCandidate).toHaveBeenCalledWith(candidateB.id);
    });
  });

  it("accepts an unchanged candidate directly", async () => {
    const candidate = createItem("candidate-direct", "Direct candidate", { status: "candidate" });
    memoryService.listMemoryItems.mockResolvedValue(createPage([candidate]));
    memoryService.getMemoryItem.mockResolvedValue(createDetail(candidate));
    memoryService.acceptMemoryCandidate.mockResolvedValue(createDetail({ ...candidate, status: "active" }));

    renderMemoryPage();
    fireEvent.click(await screen.findByRole("button", { name: "直接接受" }));

    await waitFor(() => {
      expect(memoryService.acceptMemoryCandidate).toHaveBeenCalledWith({ item_id: candidate.id });
    });
  });

  it("opens an evidence snapshot through the cross-module navigation callback", async () => {
    const item = createItem("memory-evidence", "Evidence navigation");
    const detail = createDetail(item);
    const onEvidenceOpen = vi.fn();
    memoryService.listMemoryItems.mockResolvedValue(createPage([item]));
    memoryService.getMemoryItem.mockResolvedValue(detail);

    renderMemoryPage("library", onEvidenceOpen);
    fireEvent.click(await screen.findByRole("button", { name: "来自原始对话的证据摘录" }));

    expect(onEvidenceOpen).toHaveBeenCalledWith(detail.evidence[0]);
  });
});

function renderMemoryPage(activeSubNavId = "library", onEvidenceOpen = vi.fn()) {
  return render(
    <I18nProvider>
      <MemoryPage activeSubNavId={activeSubNavId} onEvidenceOpen={onEvidenceOpen} />
    </I18nProvider>,
  );
}

function createPage(items: MemoryItem[], totalCount = items.length): MemoryItemPageResult {
  return {
    availability: "tauri",
    items,
    limit: 50,
    offset: 0,
    total_count: totalCount,
  };
}

function createDreamPreview() {
  return {
    available_session_count: 3,
    cursor_end: null,
    cursor_start: null,
    gates: [{ actual: 3, gate: "time", message: "已达到时间门禁", passed: true, reason_code: "ready", required: 3 }],
    has_more: false,
    input_char_count: 120,
    question_count: 4,
    scope: { app_id: null, project_path: null, session_id: null, source_id: null },
    scope_fingerprint: "scope:all",
    session_count: 3,
    sessions: [],
    source_revision_end: 8,
    source_revision_start: 7,
  } as const;
}

function createRecallPreview() {
  return {
    backend: "tantivy", dream_matches: [], evidence_count: 1, formal_matches: [], include_unavailable: false,
    input_char_count: 42, mode: "exact", query: "为什么使用 AppService？",
    scope: { app_id: null, project_path: null, session_id: null, source_id: null },
    selected_question_count: 1, skipped_question_count: 0, source_revision: 8,
    total_question_count: 1, truncated: false,
    questions: [{ record_kind: "session", source_id: "source-1", session_id: "session-1", session_title: "Architecture", project_path: "~/code-space/assetiweave", question_id: "question-1", question_index: 0, question_title: "Use AppService boundary", evidence_ids: ["evidence-0"], input_char_count: 42 }],
    evidence: [{ reference: "evidence-0", card_type: "answer", snapshot: { record_kind: "session", source_id: "source-1", session_id: "session-1", question_id: "question-1", turn_id: "turn-1", part_id: "part-1", block_id: "block-1", content_hash: "sha256:test", excerpt: "Use AppService for desktop and CLI.", translated_excerpt: null, event_time: "2026-07-23T00:00:00Z", source_revision: 8, source_unavailable: false } }],
  } as const;
}

function createItem(
  id: string,
  title: string,
  overrides: Partial<MemoryItem> = {},
): MemoryItem {
  return {
    confidence: 0.9,
    content_markdown: `Details for ${title}`,
    created_at: "2026-07-22T10:00:00Z",
    id,
    kind: "decision",
    origin: "manual",
    origin_dream_note_id: null,
    origin_extraction_id: null,
    origin_run_id: null,
    scope: {
      app_id: "codex",
      project_path: "~/code-space/assetiweave",
      session_id: null,
      source_id: null,
    },
    scope_fingerprint: "scope:test",
    source_revision: 8,
    stale_reason: null,
    status: "active",
    supersedes_item_id: null,
    title,
    updated_at: "2026-07-23T10:00:00Z",
    verified_revision: 8,
    ...overrides,
  };
}

function createDetail(item: MemoryItem): MemoryItemDetail {
  return {
    evidence: [
      {
        block_id: "block-1",
        content_hash: "hash-1",
        created_at: "2026-07-23T10:00:00Z",
        event_time: "2026-07-22T09:00:00Z",
        excerpt: "来自原始对话的证据摘录",
        id: "evidence-1",
        part_id: "part-1",
        question_id: "question-1",
        record_kind: "session",
        session_id: "session-1",
        source_id: "source-1",
        source_revision: 8,
        source_unavailable: false,
        translated_excerpt: null,
        turn_id: "turn-1",
        updated_at: "2026-07-23T10:00:00Z",
      },
    ],
    item,
    revisions: [
      {
        change_kind: "create",
        changed_at: "2026-07-22T10:00:00Z",
        confidence: item.confidence,
        content_markdown: item.content_markdown,
        id: "revision-1",
        item_id: item.id,
        kind: item.kind,
        origin: item.origin,
        revision_number: 1,
        scope: item.scope,
        scope_fingerprint: item.scope_fingerprint,
        source_revision: item.source_revision,
        stale_reason: item.stale_reason,
        status: item.status,
        supersedes_item_id: item.supersedes_item_id,
        title: item.title,
        verified_revision: item.verified_revision,
      },
    ],
  };
}

function createMockLocalStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: vi.fn(() => values.clear()),
    getItem: vi.fn((key: string) => values.get(key) ?? null),
    key: vi.fn((index: number) => Array.from(values.keys())[index] ?? null),
    removeItem: vi.fn((key: string) => {
      values.delete(key);
    }),
    setItem: vi.fn((key: string, value: string) => {
      values.set(key, value);
    }),
  };
}
