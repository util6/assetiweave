// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Translator } from "../../i18n/I18nProvider";
import { memoryZh } from "../../i18n/resources/zh/memory";
import type {
  MemoryProjectStateView,
  RecentProjectView,
  RecentTreeNodeView,
} from "../../types/memory";
import {
  RecentTopicTree,
  buildTreeHierarchy,
  parseNodeAnchor,
} from "./RecentTopicTree";

const mockT: Translator = (key, vars) => {
  const dict = memoryZh as Record<string, string>;
  let value = dict[key] ?? key;
  if (vars) {
    for (const [k, v] of Object.entries(vars)) {
      value = value.replace(`{${k}}`, String(v));
    }
  }
  return value;
};

function makeNode(
  partial: Partial<RecentTreeNodeView> & { id: string },
): RecentTreeNodeView {
  return {
    parentId: null,
    parentSummary: null,
    status: "completed",
    label: "已完成",
    summary: `Summary for ${partial.id}`,
    recordedAt: "2026-11-13T09:00:00Z",
    sourceSessionId: "full-session-uuid-1",
    turnIndex: 1,
    sessionTitle: "Session Alpha",
    sourceAgent: "claude",
    sessionAvailable: true,
    ...partial,
  };
}

describe("RecentTopicTree Unit & Stress Tests", () => {
  afterEach(() => {
    cleanup();
  });

  describe("parseNodeAnchor", () => {
    it("correctly parses standard shortId#T<turn>", () => {
      expect(parseNodeAnchor("098b0c4b#T13")).toEqual({
        shortId: "098b0c4b",
        turnNumber: 13,
        subIndex: undefined,
      });
    });

    it("correctly parses subindexed anchor 73c20273#T1.2", () => {
      expect(parseNodeAnchor("73c20273#T1.2")).toEqual({
        shortId: "73c20273",
        turnNumber: 1,
        subIndex: 2,
      });
    });

    it("handles turn 0 and turn 0.0", () => {
      expect(parseNodeAnchor("abc#T0")).toEqual({
        shortId: "abc",
        turnNumber: 0,
        subIndex: undefined,
      });
      expect(parseNodeAnchor("abc#T0.0")).toEqual({
        shortId: "abc",
        turnNumber: 0,
        subIndex: 0,
      });
    });

    it("returns null for invalid formats, empty strings, and missing turns", () => {
      expect(parseNodeAnchor("invalid-format")).toBeNull();
      expect(parseNodeAnchor("")).toBeNull();
      expect(parseNodeAnchor("   ")).toBeNull();
      expect(parseNodeAnchor("098b0c4b#T")).toBeNull();
      expect(parseNodeAnchor("#T13")).toBeNull();
      expect(parseNodeAnchor("098b0c4b#t13")).toBeNull();
      expect(parseNodeAnchor("098b0c4b#T1.2.3")).toBeNull();
    });
  });

  describe("buildTreeHierarchy", () => {
    it("builds a single root with children and grandchildren", () => {
      const nodes: RecentTreeNodeView[] = [
        makeNode({
          id: "root#T1",
          recordedAt: "2026-11-13T08:00:00Z",
        }),
        makeNode({
          id: "child1#T2",
          parentId: "root#T1",
          recordedAt: "2026-11-13T08:10:00Z",
        }),
        makeNode({
          id: "grandchild#T3",
          parentId: "child1#T2",
          recordedAt: "2026-11-13T08:20:00Z",
        }),
      ];

      const tree = buildTreeHierarchy(nodes);
      expect(tree).toHaveLength(1);
      expect(tree[0].id).toBe("root#T1");
      expect(tree[0].children).toHaveLength(1);
      expect(tree[0].children[0].id).toBe("child1#T2");
      expect(tree[0].children[0].children[0].id).toBe("grandchild#T3");
    });

    it("gracefully falls back to root when parentId is missing or unknown", () => {
      const nodes: RecentTreeNodeView[] = [
        makeNode({
          id: "orphan#T1",
          parentId: "missing-parent#T0",
        }),
      ];
      const tree = buildTreeHierarchy(nodes);
      expect(tree).toHaveLength(1);
      expect(tree[0].id).toBe("orphan#T1");
    });

    it("handles empty node array", () => {
      expect(buildTreeHierarchy([])).toEqual([]);
    });

    it("stress-tests deep tree hierarchy (depth 20) with branching siblings without stack overflow", () => {
      const nodes: RecentTreeNodeView[] = [];
      for (let i = 0; i <= 20; i += 1) {
        nodes.push(
          makeNode({
            id: `sess${i}#T${i + 1}`,
            parentId: i === 0 ? null : `sess${i - 1}#T${i}`,
            recordedAt: `2026-11-13T09:${String(i).padStart(2, "0")}:00Z`,
          }),
        );
        if (i > 0) {
          nodes.push(
            makeNode({
              id: `sibling${i}#T${i + 100}`,
              parentId: `sess${i - 1}#T${i}`,
              recordedAt: `2026-11-13T10:${String(i).padStart(2, "0")}:00Z`,
            }),
          );
        }
      }

      const tree = buildTreeHierarchy(nodes);
      expect(tree).toHaveLength(1);
      let curr = tree[0];
      for (let i = 1; i <= 20; i += 1) {
        expect(curr.children.length).toBeGreaterThanOrEqual(1);
        const nextMainline = curr.children.find(
          (c) => c.id === `sess${i}#T${i + 1}`,
        );
        expect(nextMainline).toBeDefined();
        curr = nextMainline!;
      }
    });

    it("handles circular references without infinite recursion", () => {
      const nodes: RecentTreeNodeView[] = [
        makeNode({ id: "cycle-a#T1", parentId: "cycle-b#T2" }),
        makeNode({ id: "cycle-b#T2", parentId: "cycle-a#T1" }),
        makeNode({ id: "self#T3", parentId: "self#T3" }),
        makeNode({ id: "valid-root#T4", parentId: null }),
        makeNode({ id: "valid-child#T5", parentId: "valid-root#T4" }),
      ];

      const roots = buildTreeHierarchy(nodes);
      expect(roots.length).toBeGreaterThanOrEqual(1);
      const validRoot = roots.find((r) => r.id === "valid-root#T4");
      expect(validRoot).toBeDefined();
      expect(validRoot?.children.map((c) => c.id)).toEqual(["valid-child#T5"]);
    });
  });

  describe("RecentTopicTree rendering and interaction", () => {
    it("renders project, topics, status badges, parent context, full session jump, and lifecycle actions", () => {
      const onNavigateSession = vi.fn();
      const onStatusChange = vi.fn();
      const onToggleProjectHistory = vi.fn();

      const project: RecentProjectView = {
        projectKey: "alpha",
        projectTitle: "Project Alpha",
        projectPath: "/path/to/alpha",
        summary: "",
        noMaterialChange: false,
        latestActivityAt: "2026-11-13T09:00:00Z",
        sourceSessionCount: 1,
        items: [],
        topics: [
          {
            topicId: "topic-1",
            topic: "Memory UI Rewrite",
            updatedAt: "2026-11-13T09:00:00Z",
            nodes: [
              makeNode({
                id: "098b0c4b#T13",
                parentId: "continued-parent#T10",
                parentSummary: "Earlier design context from continued node",
                status: "unexplored",
                label: "未探索分支",
                summary: "Decide on unexplored focus layout",
                sourceSessionId: "session-full-098b0c4b",
                turnIndex: 13,
                sessionTitle: "Design discussion",
                sourceAgent: "codex",
                sessionAvailable: true,
              }),
              makeNode({
                id: "deadbeef#T2",
                parentId: "098b0c4b#T13",
                status: "completed",
                summary: "Implemented card component",
                sourceSessionId: "session-full-deadbeef",
                turnIndex: 2,
                sessionTitle: "Missing session",
                sessionAvailable: false,
              }),
            ],
          },
        ],
      };

      render(
        <RecentTopicTree
          onNavigateSession={onNavigateSession}
          onStatusChange={onStatusChange}
          onToggleProjectHistory={onToggleProjectHistory}
          projects={[project]}
          t={mockT}
        />,
      );

      expect(screen.getByText("Project Alpha")).toBeTruthy();
      expect(screen.getByText("/path/to/alpha")).toBeTruthy();
      expect(screen.getByText("Memory UI Rewrite")).toBeTruthy();
      expect(
        screen.getByText(/Earlier design context from continued node/),
      ).toBeTruthy();
      expect(screen.getByText("codex")).toBeTruthy();
      expect(screen.getByText("Design discussion")).toBeTruthy();

      fireEvent.click(
        screen.getByRole("button", { name: "标为已完成: 098b0c4b#T13" }),
      );
      expect(onStatusChange).toHaveBeenCalledWith(
        expect.objectContaining({ id: "098b0c4b#T13" }),
        "completed",
        "alpha",
      );

      fireEvent.click(
        screen.getByRole("button", { name: "跳回 T13: 098b0c4b#T13" }),
      );
      expect(onNavigateSession).toHaveBeenCalledWith({
        record_kind: "session",
        source_id: null,
        session_id: "session-full-098b0c4b",
        question_id: null,
        part_id: null,
        turn_id: "13",
        block_id: "13",
      });

      const unavailableJump = screen.getByRole("button", {
        name: "跳回 T2: deadbeef#T2",
      }) as HTMLButtonElement;
      expect(unavailableJump.disabled).toBe(true);

      fireEvent.click(
        screen.getByRole("button", { name: "查看完整历史树" }),
      );
      expect(onToggleProjectHistory).toHaveBeenCalledWith("alpha");
    });

    it("renders full project state tree when expandedHistorySlugs includes the project", () => {
      const project: RecentProjectView = {
        projectKey: "alpha",
        projectTitle: "Project Alpha",
        projectPath: "/path/to/alpha",
        summary: "",
        noMaterialChange: false,
        latestActivityAt: "2026-11-13T10:00:00Z",
        sourceSessionCount: 1,
        items: [],
        topics: [
          {
            topicId: "topic-1",
            topic: "Topic One",
            updatedAt: "2026-11-13T10:00:00Z",
            nodes: [
              makeNode({
                id: "node-2#T5",
                parentId: "node-1#T2",
                status: "unexplored",
                summary: "Recent slice branch",
              }),
            ],
          },
        ],
      };

      const fullState: MemoryProjectStateView = {
        projectKey: "alpha",
        projectTitle: "Project Alpha",
        projectPath: "/path/to/alpha",
        updatedAt: "2026-11-13T10:00:00Z",
        watermarkCount: 9,
        topics: [
          {
            topicId: "topic-1",
            topic: "Topic One",
            updatedAt: "2026-11-13T10:00:00Z",
            nodes: [
              makeNode({
                id: "node-1#T2",
                parentId: null,
                status: "continued",
                label: "延续中",
                summary: "Continued history node",
                recordedAt: "2026-11-10T09:55:00Z",
                sourceSessionId: "full-sess-1",
                turnIndex: 2,
              }),
              makeNode({
                id: "node-2#T5",
                parentId: "node-1#T2",
                status: "unexplored",
                label: "未探索分支",
                summary: "Recent slice branch",
                recordedAt: "2026-11-13T09:55:00Z",
                sourceSessionId: "full-sess-1",
                turnIndex: 5,
              }),
            ],
          },
        ],
      };

      render(
        <RecentTopicTree
          expandedHistorySlugs={new Set(["alpha"])}
          fullProjectStates={{ alpha: fullState }}
          onToggleProjectHistory={vi.fn()}
          projects={[project]}
          t={mockT}
        />,
      );

      expect(screen.getByText("Continued history node")).toBeTruthy();
      expect(screen.getByText("延续中")).toBeTruthy();
      expect(screen.getByText(/已提炼 9 条会话/)).toBeTruthy();
      expect(screen.getByText("水位 9")).toBeTruthy();
      expect(screen.getByText("收起为近期切片")).toBeTruthy();
    });

    it("abbreviates projectPath with TruncatedText and shows abbreviated path tooltip on overflow without native title", async () => {
      const project: RecentProjectView = {
        projectKey: "alpha",
        projectTitle: "Project Alpha",
        projectPath: "/Users/demo/workspace/assetiweave",
        summary: "",
        noMaterialChange: false,
        latestActivityAt: "2026-11-13T10:00:00Z",
        sourceSessionCount: 1,
        items: [],
        topics: [
          {
            topicId: "topic-1",
            topic: "Topic One",
            updatedAt: "2026-11-13T10:00:00Z",
            nodes: [
              makeNode({
                id: "node-1#T2",
                sessionTitle: "Very long session title for truncation",
              }),
            ],
          },
        ],
      };

      render(<RecentTopicTree projects={[project]} t={mockT} />);

      const pathEl = screen.getByText("~/workspace/assetiweave");
      expect(pathEl.getAttribute("title")).toBeNull();
      Object.defineProperty(pathEl, "clientWidth", {
        configurable: true,
        get: () => 80,
      });
      Object.defineProperty(pathEl, "scrollWidth", {
        configurable: true,
        get: () => 240,
      });
      Object.defineProperty(pathEl, "clientHeight", {
        configurable: true,
        get: () => 18,
      });
      Object.defineProperty(pathEl, "scrollHeight", {
        configurable: true,
        get: () => 18,
      });

      fireEvent.mouseEnter(pathEl);
      await waitFor(() =>
        expect(screen.getByRole("tooltip").textContent).toBe(
          "~/workspace/assetiweave",
        ),
      );
      fireEvent.mouseLeave(pathEl);

      const sessionTitleEl = screen.getByText(
        "Very long session title for truncation",
      );
      expect(sessionTitleEl.getAttribute("title")).toBeNull();
    });
  });
});
