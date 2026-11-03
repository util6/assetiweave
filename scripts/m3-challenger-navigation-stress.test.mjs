import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

console.log("=== EMPIRICAL CHALLENGER STRESS-TEST: MILESTONE 3 (#167) ===");

let passed = 0;
let total = 0;
const failures = [];

function test(name, fn) {
  total++;
  process.stdout.write(`[Test ${total}] ${name} ... `);
  try {
    fn();
    passed++;
    console.log("PASS");
  } catch (err) {
    console.log("FAIL");
    console.error("   Error:", err.message);
    failures.push({ name, error: err.message });
  }
}

// -----------------------------------------------------------------------------
// 1. REPRODUCE parseNodeAnchor LOGIC FROM RecentTopicTree.tsx
// -----------------------------------------------------------------------------
// In RecentTopicTree.tsx:
// export function parseNodeAnchor(nodeId: string): NodeAnchor | null {
//   const match = nodeId.trim().match(/^([a-zA-Z0-9_-]+)#T(\d+)(?:\.(\d+))?$/);
//   if (!match) return null;
//   return {
//     shortId: match[1],
//     turnNumber: parseInt(match[2], 10),
//     subIndex: match[3] ? parseInt(match[3], 10) : undefined,
//   };
// }

function parseNodeAnchorCurrent(nodeId) {
  const match = nodeId.trim().match(/^([a-zA-Z0-9_-]+)#T(\d+)(?:\.(\d+))?$/);
  if (!match) return null;
  return {
    shortId: match[1],
    turnNumber: parseInt(match[2], 10),
    subIndex: match[3] ? parseInt(match[3], 10) : undefined,
  };
}

// -----------------------------------------------------------------------------
// TEST SUITE 1: Diverse Node Anchor Formats
// -----------------------------------------------------------------------------
test("parseNodeAnchor: standard node ID '098b0c4b#T13'", () => {
  const res = parseNodeAnchorCurrent("098b0c4b#T13");
  assert.deepEqual(res, { shortId: "098b0c4b", turnNumber: 13, subIndex: undefined });
});

test("parseNodeAnchor: sub-index node ID '73c20273#T1.2'", () => {
  const res = parseNodeAnchorCurrent("73c20273#T1.2");
  assert.deepEqual(res, { shortId: "73c20273", turnNumber: 1, subIndex: 2 });
});

test("parseNodeAnchor: turn zero '098b0c4b#T0'", () => {
  const res = parseNodeAnchorCurrent("098b0c4b#T0");
  assert.deepEqual(res, { shortId: "098b0c4b", turnNumber: 0, subIndex: undefined });
});

test("parseNodeAnchor: turn zero with subindex '098b0c4b#T0.0'", () => {
  const res = parseNodeAnchorCurrent("098b0c4b#T0.0");
  assert.deepEqual(res, { shortId: "098b0c4b", turnNumber: 0, subIndex: 0 });
});

test("parseNodeAnchor: alphanumeric shortId with dashes and underscores 'session-id_foo#T42.1'", () => {
  const res = parseNodeAnchorCurrent("session-id_foo#T42.1");
  assert.deepEqual(res, { shortId: "session-id_foo", turnNumber: 42, subIndex: 1 });
});

test("parseNodeAnchor: leading and trailing whitespace '  098b0c4b#T13  '", () => {
  const res = parseNodeAnchorCurrent("  098b0c4b#T13  ");
  assert.deepEqual(res, { shortId: "098b0c4b", turnNumber: 13, subIndex: undefined });
});

test("parseNodeAnchor: malformed 'invalid-format'", () => {
  assert.strictEqual(parseNodeAnchorCurrent("invalid-format"), null);
});

test("parseNodeAnchor: empty string ''", () => {
  assert.strictEqual(parseNodeAnchorCurrent(""), null);
});

test("parseNodeAnchor: whitespace only '   '", () => {
  assert.strictEqual(parseNodeAnchorCurrent("   "), null);
});

test("parseNodeAnchor: missing turn number '098b0c4b#T'", () => {
  assert.strictEqual(parseNodeAnchorCurrent("098b0c4b#T"), null);
});

test("parseNodeAnchor: missing shortId '#T13'", () => {
  assert.strictEqual(parseNodeAnchorCurrent("#T13"), null);
});

test("parseNodeAnchor: lowercase t '098b0c4b#t13'", () => {
  // Spec requires #T, lowercase should not match
  assert.strictEqual(parseNodeAnchorCurrent("098b0c4b#t13"), null);
});

test("parseNodeAnchor: negative turn '098b0c4b#T-5'", () => {
  assert.strictEqual(parseNodeAnchorCurrent("098b0c4b#T-5"), null);
});

test("parseNodeAnchor: multiple subindices '098b0c4b#T1.2.3'", () => {
  assert.strictEqual(parseNodeAnchorCurrent("098b0c4b#T1.2.3"), null);
});

test("parseNodeAnchor: edge case null (CURRENT BEHAVIOR CHECK)", () => {
  let threw = false;
  try {
    parseNodeAnchorCurrent(null);
  } catch (err) {
    threw = true;
  }
  // In TypeScript it's typed string, but at runtime passing null throws TypeError because of .trim()
  assert.ok(threw, "Current implementation throws on null instead of returning null");
});

test("parseNodeAnchor: edge case undefined (CURRENT BEHAVIOR CHECK)", () => {
  let threw = false;
  try {
    parseNodeAnchorCurrent(undefined);
  } catch (err) {
    threw = true;
  }
  assert.ok(threw, "Current implementation throws on undefined instead of returning null");
});

// -----------------------------------------------------------------------------
// 2. REPRODUCE AND TEST resolveConversationNavigationTarget FROM ConversationsPage.tsx
// -----------------------------------------------------------------------------
function resolveConversationNavigationTarget(session, target) {
  const candidateQuestions = target.questionId
    ? session.questions.filter(
        (question) => question.question.id === target.questionId,
      )
    : session.questions;
  if (candidateQuestions.length === 0) return null;
  if (!target.blockId) {
    return {
      blockFound: true,
      blockId: null,
      cardType: null,
      questionId: candidateQuestions[0].question.id,
    };
  }

  for (const question of candidateQuestions) {
    const turnBlockMatch = question.turns.some((turn) => {
      if (`${turn.id}-question` === target.blockId || turn.id === target.blockId) {
        return true;
      }
      if (target.blockId != null) {
        const numeric = Number(target.blockId);
        if (!Number.isNaN(numeric) && turn.turn_index === numeric) {
          return true;
        }
        if (target.blockId.startsWith("T")) {
          const tNum = Number(target.blockId.slice(1));
          if (!Number.isNaN(tNum) && turn.turn_index === tNum) {
            return true;
          }
        }
      }
      return false;
    });

    if (turnBlockMatch) {
      return {
        blockFound: true,
        blockId: target.blockId,
        cardType: "question",
        questionId: question.question.id,
      };
    }
    const block = question.projected_content_nodes.find(
      (candidate) =>
        candidate.node_id === target.blockId ||
        candidate.legacy_anchor_ids.includes(target.blockId ?? ""),
    );
    if (block) {
      return {
        blockFound: true,
        blockId: block.node_id,
        cardType: block.node_type,
        questionId: question.question.id,
      };
    }
  }

  return {
    blockFound: false,
    blockId: null,
    cardType: null,
    questionId: candidateQuestions[0].question.id,
  };
}

const mockSession = {
  session: { id: "098b0c4b-1234-5678-abcd-111122223333" },
  questions: [
    {
      question: { id: "q-root" },
      turns: [
        { id: "turn-0", turn_index: 0 },
        { id: "turn-1", turn_index: 1 },
        { id: "turn-13", turn_index: 13 },
      ],
      projected_content_nodes: [
        {
          node_id: "node-plan",
          node_type: "plan",
          legacy_anchor_ids: ["legacy-plan-1"],
        },
      ],
    },
    {
      question: { id: "q-followup" },
      turns: [
        { id: "turn-14", turn_index: 14 },
      ],
      projected_content_nodes: [],
    },
  ],
};

test("resolveConversationNavigationTarget: matches numeric turn index '13'", () => {
  const res = resolveConversationNavigationTarget(mockSession, {
    sessionId: "098b0c4b",
    blockId: "13",
    recordKind: "session",
    nonce: "n1",
  });
  assert.strictEqual(res.blockFound, true);
  assert.strictEqual(res.blockId, "13");
  assert.strictEqual(res.cardType, "question");
  assert.strictEqual(res.questionId, "q-root");
});

test("resolveConversationNavigationTarget: matches 'T13' turn prefix", () => {
  const res = resolveConversationNavigationTarget(mockSession, {
    sessionId: "098b0c4b",
    blockId: "T13",
    recordKind: "session",
    nonce: "n2",
  });
  assert.strictEqual(res.blockFound, true);
  assert.strictEqual(res.blockId, "T13");
  assert.strictEqual(res.cardType, "question");
  assert.strictEqual(res.questionId, "q-root");
});

test("resolveConversationNavigationTarget: matches turn 0 with '0'", () => {
  const res = resolveConversationNavigationTarget(mockSession, {
    sessionId: "098b0c4b",
    blockId: "0",
    recordKind: "session",
    nonce: "n3",
  });
  assert.strictEqual(res.blockFound, true);
  assert.strictEqual(res.blockId, "0");
  assert.strictEqual(res.cardType, "question");
  assert.strictEqual(res.questionId, "q-root");
});

test("resolveConversationNavigationTarget: matches turn 0 with 'T0'", () => {
  const res = resolveConversationNavigationTarget(mockSession, {
    sessionId: "098b0c4b",
    blockId: "T0",
    recordKind: "session",
    nonce: "n4",
  });
  assert.strictEqual(res.blockFound, true);
  assert.strictEqual(res.blockId, "T0");
  assert.strictEqual(res.cardType, "question");
  assert.strictEqual(res.questionId, "q-root");
});

test("resolveConversationNavigationTarget: matches turn in secondary question '14'", () => {
  const res = resolveConversationNavigationTarget(mockSession, {
    sessionId: "098b0c4b",
    blockId: "14",
    recordKind: "session",
    nonce: "n5",
  });
  assert.strictEqual(res.blockFound, true);
  assert.strictEqual(res.blockId, "14");
  assert.strictEqual(res.cardType, "question");
  assert.strictEqual(res.questionId, "q-followup");
});

test("resolveConversationNavigationTarget: non-existent turn index '99' reports blockFound=false", () => {
  const res = resolveConversationNavigationTarget(mockSession, {
    sessionId: "098b0c4b",
    blockId: "99",
    recordKind: "session",
    nonce: "n6",
  });
  assert.strictEqual(res.blockFound, false);
  assert.strictEqual(res.blockId, null);
  assert.strictEqual(res.questionId, "q-root");
});

test("resolveConversationNavigationTarget: matches projected node 'node-plan'", () => {
  const res = resolveConversationNavigationTarget(mockSession, {
    sessionId: "098b0c4b",
    blockId: "node-plan",
    recordKind: "session",
    nonce: "n7",
  });
  assert.strictEqual(res.blockFound, true);
  assert.strictEqual(res.blockId, "node-plan");
  assert.strictEqual(res.cardType, "plan");
  assert.strictEqual(res.questionId, "q-root");
});

test("resolveConversationNavigationTarget: matches legacy anchor 'legacy-plan-1'", () => {
  const res = resolveConversationNavigationTarget(mockSession, {
    sessionId: "098b0c4b",
    blockId: "legacy-plan-1",
    recordKind: "session",
    nonce: "n8",
  });
  assert.strictEqual(res.blockFound, true);
  assert.strictEqual(res.blockId, "node-plan");
  assert.strictEqual(res.cardType, "plan");
  assert.strictEqual(res.questionId, "q-root");
});

test("resolveConversationNavigationTarget: empty blockId returns first question", () => {
  const res = resolveConversationNavigationTarget(mockSession, {
    sessionId: "098b0c4b",
    blockId: "",
    recordKind: "session",
    nonce: "n9",
  });
  assert.strictEqual(res.blockFound, true);
  assert.strictEqual(res.blockId, null);
  assert.strictEqual(res.questionId, "q-root");
});

test("resolveConversationNavigationTarget: edge case blockId=' ' (whitespace)", () => {
  // Notice: Number(" ") === 0. If there is a turn with turn_index = 0, Number(" ") matches 0!
  const res = resolveConversationNavigationTarget(mockSession, {
    sessionId: "098b0c4b",
    blockId: " ",
    recordKind: "session",
    nonce: "n10",
  });
  // Empirically verify whether Number(" ") === 0 matches turn 0
  const matchesTurn0 = (res.blockFound === true && res.questionId === "q-root");
  assert.ok(matchesTurn0, "Notice: Number(' ') === 0 matches turn_index 0 in current implementation");
});

// -----------------------------------------------------------------------------
// 3. SESSION MATCHING LOGIC IN ConversationsPage.tsx
// -----------------------------------------------------------------------------
function conversationIdFragment(value) {
  const normalized = value.trim().toLowerCase();
  const conversationHashPattern = /(?:^|[^a-f0-9])([a-f0-9]{64})(?![a-f0-9])/i;
  const legacyConversationHashPattern = /(?:^|[^a-f0-9])([a-f0-9]{12,})(?![a-f0-9])/i;
  const hash =
    normalized.match(conversationHashPattern)?.[1] ??
    normalized.match(legacyConversationHashPattern)?.[1];
  const fragment = (hash ?? normalized).slice(0, 8);
  const nodeOrder = normalized.match(/-node-(\d+)$/)?.[1];
  return nodeOrder === undefined ? fragment : `${fragment}:n${nodeOrder}`;
}

const sessionsList = [
  { id: "098b0c4b-1111-2222-3333-444455556666", adapter_id: "antigravity" },
  { id: "73c20273-aaaa-bbbb-cccc-ddddeeeeffff", adapter_id: "codex" },
  { id: "custom-session-slug-12345", adapter_id: "claude-code" },
];

function findTargetSession(sessions, targetSessionId) {
  return sessions.find(
    (session) =>
      session.id === targetSessionId ||
      conversationIdFragment(session.id) === targetSessionId ||
      session.id.startsWith(targetSessionId),
  );
}

test("findTargetSession: matches by full session ID", () => {
  const found = findTargetSession(sessionsList, "098b0c4b-1111-2222-3333-444455556666");
  assert.strictEqual(found?.id, "098b0c4b-1111-2222-3333-444455556666");
});

test("findTargetSession: matches by 8-char short_id '098b0c4b'", () => {
  const found = findTargetSession(sessionsList, "098b0c4b");
  assert.strictEqual(found?.id, "098b0c4b-1111-2222-3333-444455556666");
});

test("findTargetSession: matches second session by short_id '73c20273'", () => {
  const found = findTargetSession(sessionsList, "73c20273");
  assert.strictEqual(found?.id, "73c20273-aaaa-bbbb-cccc-ddddeeeeffff");
});

test("findTargetSession: matches prefix for slug session 'custom-session'", () => {
  const found = findTargetSession(sessionsList, "custom-session");
  assert.strictEqual(found?.id, "custom-session-slug-12345");
});

test("findTargetSession: non-existent short_id 'deadbeef' returns undefined", () => {
  const found = findTargetSession(sessionsList, "deadbeef");
  assert.strictEqual(found, undefined);
});

test("findTargetSession: edge case empty string '' matches FIRST session due to startsWith('')", () => {
  const found = findTargetSession(sessionsList, "");
  // In JavaScript, "any_string".startsWith("") is true.
  assert.strictEqual(found?.id, sessionsList[0].id, "Empty string unexpectedly matches first session");
});

// -----------------------------------------------------------------------------
// 4. UI THEME TOKEN AND STYLING CONFORMANCE CHECK
// -----------------------------------------------------------------------------
const recentTopicTreePath = path.resolve(
  import.meta.dirname,
  "../frontend/src/components/memory/RecentTopicTree.tsx"
);
const recentTopicTreeCode = fs.readFileSync(recentTopicTreePath, "utf8");

test("RecentTopicTree: Zero rounded-sm", () => {
  const hasRoundedSm = /\brounded-sm\b/.test(recentTopicTreeCode);
  assert.strictEqual(hasRoundedSm, false, "Must not contain rounded-sm");
});

test("RecentTopicTree: Zero rounded-md", () => {
  const hasRoundedMd = /\brounded-md\b/.test(recentTopicTreeCode);
  assert.strictEqual(hasRoundedMd, false, "Must not contain rounded-md");
});

test("RecentTopicTree: Zero raw hex colors (#...)", () => {
  // Look for inline hex colors like #fff, #123456 in className or style
  const hexMatches = recentTopicTreeCode.match(/#([0-9a-fA-F]{3,8})\b/g);
  // Exclude node anchor format #T
  const colorHexMatches = hexMatches ? hexMatches.filter(h => !h.startsWith("#T")) : [];
  assert.strictEqual(colorHexMatches.length, 0, `Found raw hex colors: ${colorHexMatches.join(", ")}`);
});

test("RecentTopicTree: Uses rounded-2xl for panels and sections", () => {
  assert.ok(recentTopicTreeCode.includes("rounded-2xl"), "Must use rounded-2xl");
});

test("RecentTopicTree: Uses rounded-xl for cards", () => {
  assert.ok(recentTopicTreeCode.includes("rounded-xl"), "Must use rounded-xl");
});

test("RecentTopicTree: Uses rounded-full for pills and chips", () => {
  assert.ok(recentTopicTreeCode.includes("rounded-full"), "Must use rounded-full");
});

// Check MemoryRecentWorkspace.tsx for rounded-sm / rounded-md
const memoryRecentWorkspacePath = path.resolve(
  import.meta.dirname,
  "../frontend/src/components/memory/MemoryRecentWorkspace.tsx"
);
const memoryRecentWorkspaceCode = fs.readFileSync(memoryRecentWorkspacePath, "utf8");

test("MemoryRecentWorkspace: Check rounded-sm", () => {
  const hasRoundedSm = /\brounded-sm\b/.test(memoryRecentWorkspaceCode);
  assert.strictEqual(hasRoundedSm, false, "Must not contain rounded-sm");
});

test("MemoryRecentWorkspace: Audit rounded-md", () => {
  const roundedMdMatches = memoryRecentWorkspaceCode.match(/\brounded-md\b/g);
  if (roundedMdMatches) {
    console.log(`\n      [AUDIT NOTE] MemoryRecentWorkspace.tsx contains ${roundedMdMatches.length} instance(s) of rounded-md (lines 499 & 851 in legacy items view)`);
  }
  // We record this observation for handoff report
});

console.log("\n=======================================================");
console.log(`SUMMARY: ${passed} passed, ${failures.length} failed out of ${total} tests.`);
if (failures.length > 0) {
  console.log("FAILURES:");
  failures.forEach(f => console.log(` - ${f.name}: ${f.error}`));
}
console.log("=======================================================");
