const assert = require("node:assert/strict");
const test = require("node:test");

const {
  normalizeTurn
} = require("../templates/gemini-web/scripts/gemini-normalize.cjs");

test("keeps Gemini assistant-only artifact turns", () => {
  const turn = normalizeTurn("c_demo", [
    ["c_demo", "r_artifact"],
    null,
    [[""], 2, null, 0, "client-id"],
    [[
      [
        "rc_demo",
        ["Here is the generated app.\n\nhttp://googleusercontent.com/immersive_entry_chip/0"],
        null,
        null,
        null,
        null,
        true,
        null,
        [2],
        "zh",
        null,
        null,
        [],
        null,
        null,
        true,
        null,
        null,
        null,
        null,
        null,
        [false],
        null,
        null,
        null,
        null,
        null,
        null,
        [],
        null,
        [[
          "demo.html",
          "artifact-id",
          "Demo",
          null,
          "```html\n<!doctype html>\n<html><body>ok</body></html>\n```"
        ]]
      ]
    ]]
  ], 0);

  assert.equal(turn.user_text, "[Gemini continuation without visible user prompt]");
  assert.deepEqual(
    turn.parts.map((part) => [part.kind, part.language, part.text]),
    [
      ["text", null, "Here is the generated app."],
      ["code_block", "html", "<!doctype html>\n<html><body>ok</body></html>"]
    ]
  );
});

test("captures Gemini generated media links", () => {
  const turn = normalizeTurn("c_demo", [
    ["c_demo", "r_image"],
    null,
    [["draw this"], 2, null, 0, "client-id"],
    [[
      [
        "rc_demo",
        ["Done."],
        null,
        null,
        null,
        null,
        true,
        null,
        [2],
        "zh",
        null,
        null,
        [[
          [["ignored"], "generated.png", null, "https://lh3.googleusercontent.com/gg/example", null, null, null, null, null, null, null, "image/png"]
        ]]
      ]
    ]]
  ], 0);

  assert.equal(turn.user_text, "draw this");
  assert.deepEqual(
    turn.parts.map((part) => part.text),
    [
      "Done.",
      "Gemini media:\n- [generated.png](https://lh3.googleusercontent.com/gg/example) (image/png)"
    ]
  );
});

test("adds user attachment references to the question text", () => {
  const turn = normalizeTurn("c_demo", [
    ["c_demo", "r_upload"],
    null,
    [["explain this", null, null, null, [[null, null, "input.png", "https://lh3.googleusercontent.com/gg/uploaded", null, null, null, null, null, null, null, "image/png"]]]],
    [[["rc_demo", ["Answer."]]]]
  ], 0);

  assert.equal(
    turn.user_text,
    "explain this\n\nUser attachments:\n- [input.png](https://lh3.googleusercontent.com/gg/uploaded) (image/png)"
  );
  assert.equal(turn.parts[0].text, "Answer.");
});
