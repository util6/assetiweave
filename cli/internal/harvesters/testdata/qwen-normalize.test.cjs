const assert = require("node:assert/strict");
const test = require("node:test");

const {
  normalizeRound
} = require("../templates/qwen-web/scripts/qwen-normalize.cjs");

test("normalizes current Qwen assistant text messages", () => {
  const turn = normalizeRound({
    req_id: "request-current",
    request_messages: [
      { content: "How does this work?" }
    ],
    qwen_response_messages: [
      { role: "plugin", contentType: "plugin", content: "{\"result\":\"noise\"}" },
      { role: "assistant", contentType: "think", content: "{\"content\":\"private reasoning\"}" },
      { role: "assistant", contentType: "text", content: "First answer section." },
      { role: "assistant", contentType: "referenceLink", content: "{\"links\":[]}" },
      { role: "assistant", contentType: "text", content: "Second answer section." },
      { role: "assistant", contentType: "card", content: "{\"title\":\"artifact\"}" }
    ]
  }, 7);

  assert.equal(turn.external_id, "request-current");
  assert.equal(turn.turn_index, 7);
  assert.equal(turn.user_text, "How does this work?");
  assert.deepEqual(
    turn.parts.map((part) => part.text),
    ["First answer section.", "Second answer section."]
  );
});

test("normalizes legacy Qwen final answer messages", () => {
  const turn = normalizeRound({
    req_id: "request-legacy",
    request_messages: [
      { content: "Legacy question" }
    ],
    response_messages: [
      { mime_type: "bar/progress", content: "Search complete" },
      { mime_type: "multi_load/iframe", content: "Legacy final answer." }
    ]
  }, 0);

  assert.deepEqual(
    turn.parts.map((part) => part.text),
    ["Legacy final answer."]
  );
});

test("keeps questions whose response was interrupted", () => {
  const turn = normalizeRound({
    req_id: "request-interrupted",
    request_messages: [
      { content: "Question with interrupted response" }
    ],
    qwen_response_messages: [
      { role: "plugin", contentType: "plugin", status: "interrupted", content: "{}" }
    ]
  }, 0);

  assert.equal(turn.user_text, "Question with interrupted response");
  assert.deepEqual(turn.parts, []);
});
