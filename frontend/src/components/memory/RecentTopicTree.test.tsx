/* @vitest-environment jsdom */

import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import {
  RecentTopicTree,
  buildTreeHierarchy,
  parseNodeAnchor,
} from "./RecentTopicTree";
import type {
  RecentProjectView,
  RecentTopicView,
  RecentTreeNodeView,
} from "../../types/memory";
import type { Translator } from "../../i18n/I18nProvider";

const mockT: Translator = (key: string) => key;

describe("RecentTopicTree Unit & Stress Tests", () => {
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
      const nodes = [
        {
          id: "root#T1",
          parentId: null,
          status: "completed" as const,
          label: "Root Node",
          summary: "Root summary",
          recordedAt: "2026-11-02T10:00:00Z",
        },
        {
          id: "child1#T2",
          parentId: "root#T1",
          status: "continued" as const,
          label: "Child 1",
          summary: "Child 1 summary",
          recordedAt: "2026-11-02T10:30:00Z",
        },
        {
          id: "grandchild#T3",
          parentId: "child1#T2",
          status: "unexplored" as const,
          label: "Grandchild",
          summary: "Grandchild summary",
          recordedAt: "2026-11-02T11:00:00Z",
        },
      ];

      const tree = buildTreeHierarchy(nodes);
      expect(tree).toHaveLength(1);
      expect(tree[0].id).toBe("root#T1");
      expect(tree[0].children).toHaveLength(1);
      expect(tree[0].children[0].id).toBe("child1#T2");
      expect(tree[0].children[0].children).toHaveLength(1);
      expect(tree[0].children[0].children[0].id).toBe("grandchild#T3");
    });

    it("gracefully falls back to root when parentId is missing or unknown", () => {
      const nodes = [
        {
          id: "orphan#T1",
          parentId: "non-existent-parent",
          status: "completed" as const,
          label: "Orphan Node",
          summary: "Orphan summary",
          recordedAt: "2026-11-02T10:00:00Z",
        },
      ];

      const tree = buildTreeHierarchy(nodes);
      expect(tree).toHaveLength(1);
      expect(tree[0].id).toBe("orphan#T1");
    });

    it("handles empty node array", () => {
      expect(buildTreeHierarchy([])).toEqual([]);
    });
  });

  describe("RecentTopicTree rendering and interaction", () => {
    it("renders project, topics, multi-way hierarchy, badges, and lineage notes", () => {
      const onNavigateSession = vi.fn();
      const topic: RecentTopicView = {
        topicId: "topic-1",
        topic: "Core Engine Refactor",
        updatedAt: "2026-11-02T12:00:00Z",
        nodes: [
          {
            id: "098b0c4b#T13",
            parentId: null,
            status: "completed",
            label: "Node 1",
            summary: "Root completed node",
            recordedAt: "2026-11-02T10:00:00Z",
          },
          {
            id: "73c20273#T1.2",
            parentId: "098b0c4b#T13",
            status: "continued",
            label: "Node 2",
            summary: "Child continued node",
            recordedAt: "2026-11-02T11:00:00Z",
          },
          {
            id: "malformed-node-id",
            parentId: "098b0c4b#T13",
            status: "unexplored",
            label: "Node 3",
            summary: "Malformed node ID without anchor",
            recordedAt: "2026-11-02T11:30:00Z",
          },
        ],
      };

      const project: RecentProjectView = {
        projectKey: "proj-1",
        projectTitle: "Project Alpha",
        projectPath: "/path/to/alpha",
        summary: "Project summary",
        noMaterialChange: false,
        latestActivityAt: "2026-11-02T12:00:00Z",
        sourceSessionCount: 2,
        items: [],
        topics: [topic],
      };

      render(
        <RecentTopicTree
          onNavigateSession={onNavigateSession}
          project={project}
          t={mockT}
        />,
      );

      // Verify headers
      expect(screen.getByText("Project Alpha")).toBeTruthy();
      expect(screen.getByText("/path/to/alpha")).toBeTruthy();
      expect(screen.getByText("topic-1")).toBeTruthy();
      expect(screen.getByText("Core Engine Refactor")).toBeTruthy();

      // Verify status counts in header
      expect(screen.getByText("1 已完成")).toBeTruthy();
      expect(screen.getByText("1 推进中")).toBeTruthy();
      expect(screen.getByText("1 待探索")).toBeTruthy();

      // Verify lineage note
      expect(screen.getAllByText(/\(↖ 继承自 098b0c4b#T13\)/)).toHaveLength(2);

      // Verify jump buttons for valid anchors
      const jump1 = screen.getByRole("button", { name: /098b0c4b #T13/ });
      expect(jump1).toBeTruthy();
      fireEvent.click(jump1);

      expect(onNavigateSession).toHaveBeenCalledTimes(1);
      expect(onNavigateSession).toHaveBeenCalledWith({
        record_kind: "session",
        source_id: null,
        session_id: "098b0c4b",
        question_id: null,
        turn_id: "13",
        part_id: null,
        block_id: "13",
      });

      // Verify sub-index jump button
      const jump2 = screen.getByRole("button", { name: /73c20273 #T1/ });
      expect(jump2).toBeTruthy();
      fireEvent.click(jump2);

      expect(onNavigateSession).toHaveBeenCalledTimes(2);
      expect(onNavigateSession).toHaveBeenCalledWith({
        record_kind: "session",
        source_id: null,
        session_id: "73c20273",
        question_id: null,
        turn_id: "1",
        part_id: null,
        block_id: "1",
      });

      // Verify malformed-node-id does NOT have a jump button
      const allJumpButtons = screen.getAllByRole("button");
      const hasMalformedButton = allJumpButtons.some((b) =>
        b.textContent?.includes("malformed-node-id"),
      );
      expect(hasMalformedButton).toBe(false);
    });

    it("handles empty topics array gracefully", () => {
      const project: RecentProjectView = {
        projectKey: "proj-empty",
        projectTitle: "Empty Topics",
        projectPath: null,
        summary: "",
        noMaterialChange: false,
        latestActivityAt: "2026-11-02T12:00:00Z",
        sourceSessionCount: 0,
        items: [],
        topics: [],
      };

      const { container } = render(
        <RecentTopicTree project={project} t={mockT} />,
      );
      expect(container.textContent).toContain("Empty Topics");
      expect(container.textContent).toContain("0 memory.recent.topics");
    });

    it("stress-tests deep tree hierarchy (depth 20) with branching siblings without stack overflow", () => {
      const onNavigateSession = vi.fn();
      const nodes: RecentTreeNodeView[] = [];
      const DEPTH = 20;

      // Root
      nodes.push({
        id: "sess0#T1",
        parentId: null,
        status: "completed",
        label: "Root Level 0",
        summary: "Root summary",
        recordedAt: "2026-11-02T10:00:00Z",
      });

      // Chain of 20 nodes with siblings
      for (let d = 1; d <= DEPTH; d++) {
        const parentId = `sess${d - 1}#T1`;
        nodes.push({
          id: `sess${d}#T1`,
          parentId,
          status: d % 3 === 0 ? "completed" : d % 3 === 1 ? "continued" : "unexplored",
          label: `Chain Level ${d}`,
          summary: `Summary at depth ${d}`,
          recordedAt: "2026-11-02T10:00:00Z",
        });

        if (d % 2 === 0) {
          nodes.push({
            id: `sess${d}#T1.sib`,
            parentId,
            status: "unexplored",
            label: `Sibling Level ${d}`,
            summary: `Sibling at depth ${d}`,
            recordedAt: "2026-11-02T10:00:00Z",
          });
        }
      }

      const project: RecentProjectView = {
        projectKey: "deep-stress-proj",
        projectTitle: "Deep Tree Stress",
        projectPath: "/path/to/deep",
        summary: "Testing depth 20 hierarchy",
        noMaterialChange: false,
        latestActivityAt: "2026-11-02T12:00:00Z",
        sourceSessionCount: 21,
        items: [],
        topics: [
          {
            topicId: "topic-deep",
            topic: "Deep Hierarchy Topic",
            updatedAt: "2026-11-02T12:00:00Z",
            nodes,
          },
        ],
      };

      // 1. Test buildTreeHierarchy directly
      const roots = buildTreeHierarchy(nodes);
      expect(roots).toHaveLength(1);

      let curr = roots[0];
      for (let d = 1; d <= DEPTH; d++) {
        expect(curr.children.length).toBeGreaterThanOrEqual(1);
        const nextMainline = curr.children.find((c) => c.id === `sess${d}#T1`);
        expect(nextMainline).toBeDefined();
        curr = nextMainline!;
      }

      // 2. Test React rendering under depth 20
      render(
        <RecentTopicTree
          onNavigateSession={onNavigateSession}
          project={project}
          t={mockT}
        />,
      );

      expect(screen.getByText("Root Level 0")).toBeTruthy();
      expect(screen.getByText("Chain Level 20")).toBeTruthy();
      expect(screen.getByText("Sibling Level 20")).toBeTruthy();

      // Test jump button at deepest level
      const deepJump = screen.getByRole("button", { name: /sess20 #T1/ });
      expect(deepJump).toBeTruthy();
      fireEvent.click(deepJump);
      expect(onNavigateSession).toHaveBeenCalledWith({
        record_kind: "session",
        source_id: null,
        session_id: "sess20",
        question_id: null,
        turn_id: "1",
        part_id: null,
        block_id: "1",
      });
    });

    it("stress-tests circular references (A <-> B and self-referencing) without hanging or crashing", () => {
      const nodes: RecentTreeNodeView[] = [
        {
          id: "cycle-a#T1",
          parentId: "cycle-b#T2",
          status: "continued",
          label: "Cycle A",
          summary: "Points to B",
        },
        {
          id: "cycle-b#T2",
          parentId: "cycle-a#T1",
          status: "continued",
          label: "Cycle B",
          summary: "Points to A",
        },
        {
          id: "self-cycle#T3",
          parentId: "self-cycle#T3",
          status: "unexplored",
          label: "Self Cycle",
          summary: "Points to self",
        },
        {
          id: "valid-root#T4",
          parentId: null,
          status: "completed",
          label: "Valid Root Alongside Cycle",
          summary: "Legitimate root",
        },
        {
          id: "valid-child#T5",
          parentId: "valid-root#T4",
          status: "continued",
          label: "Valid Child Alongside Cycle",
          summary: "Legitimate child",
        },
      ];

      const roots = buildTreeHierarchy(nodes);
      // Only the valid root should be in roots; cycles are safely excluded from roots
      expect(roots).toHaveLength(1);
      expect(roots[0].id).toBe("valid-root#T4");
      expect(roots[0].children).toHaveLength(1);
      expect(roots[0].children[0].id).toBe("valid-child#T5");

      const project: RecentProjectView = {
        projectKey: "cycle-proj",
        projectTitle: "Cycle Resilience Project",
        projectPath: "/path/to/cycle",
        summary: "Project with circular nodes",
        noMaterialChange: false,
        latestActivityAt: "2026-11-02T12:00:00Z",
        sourceSessionCount: 2,
        items: [],
        topics: [
          {
            topicId: "topic-cycle",
            topic: "Cycle Resilience Topic",
            updatedAt: "2026-11-02T12:00:00Z",
            nodes,
          },
        ],
      };

      // Ensure React renders cleanly without recursion error
      render(<RecentTopicTree project={project} t={mockT} />);
      expect(screen.getByText("Valid Root Alongside Cycle")).toBeTruthy();
      expect(screen.getByText("Valid Child Alongside Cycle")).toBeTruthy();
    });
  });
});
