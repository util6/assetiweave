#!/usr/bin/env node

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import {
  countLines,
  MAX_LINES,
  SOFT_LINE_LIMIT,
} from "./check-backend-line-limits.mjs";

// 规模门禁的语义：500 是关注线（只提示），1000 是硬上限（直接失败）。
assert.equal(SOFT_LINE_LIMIT, 500);
assert.equal(MAX_LINES, 1000);

// Unit test countLines directly
assert.equal(countLines(""), 0);
assert.equal(countLines("pub fn foo() {}\n".repeat(500)), 500);
assert.equal(countLines("pub fn foo() {}\r\n".repeat(500)), 500);
assert.equal(countLines("pub fn foo() {}\n".repeat(500).slice(0, -1)), 500);
assert.equal(countLines("pub fn foo() {}\n".repeat(501)), 501);

const scriptPath = path.resolve("scripts/check-backend-line-limits.mjs");
const fixtureRoot = fs.mkdtempSync(path.join(os.tmpdir(), "assetiweave-line-guard-"));

const runScript = () =>
  execFileSync(process.execPath, [scriptPath, fixtureRoot], {
    encoding: "utf8",
  });

try {
  const backendDir = path.join(fixtureRoot, "src-tauri/src/backend/domain");
  fs.mkdirSync(backendDir, { recursive: true });
  const validFile = path.join(backendDir, "valid.rs");

  // 1. 关注线以下：通过，且不出现任何提示。
  fs.writeFileSync(validFile, "pub fn foo() {}\n".repeat(100));

  const smallOutput = runScript();
  assert.ok(smallOutput.includes("backend line limits passed"));
  assert.ok(smallOutput.includes(`0 exceed ${MAX_LINES} lines`));
  assert.ok(!smallOutput.includes("notice"));

  // 2. 关注线与硬上限之间：通过，但要给出拆分提示。
  fs.writeFileSync(validFile, "pub fn foo() {}\n".repeat(600));

  const softOutput = runScript();
  assert.ok(softOutput.includes("backend line limits passed"));
  assert.ok(softOutput.includes("backend line limits notice"));
  assert.ok(softOutput.includes("600 lines"));
  assert.ok(softOutput.includes("valid.rs"));

  // 3. 恰好等于硬上限：仍然通过（上限是 1000，不是 999）。
  fs.writeFileSync(validFile, "pub fn foo() {}\n".repeat(MAX_LINES));

  const boundaryOutput = runScript();
  assert.ok(boundaryOutput.includes("backend line limits passed"));
  assert.ok(boundaryOutput.includes("backend line limits notice"));

  // 4. 越过硬上限一行：直接失败。
  const longFile = path.join(backendDir, "long.rs");
  fs.writeFileSync(longFile, "pub fn bar() {}\n".repeat(MAX_LINES + 1));

  let failed = false;
  try {
    execFileSync(process.execPath, [scriptPath, fixtureRoot], {
      encoding: "utf8",
      stdio: "pipe",
    });
  } catch (err) {
    failed = true;
    const stderr = err.stderr?.toString() ?? "";
    assert.ok(
      stderr.includes(`1 production file(s) exceed ${MAX_LINES} lines`),
    );
    assert.ok(stderr.includes(`${MAX_LINES + 1} lines`));
    assert.ok(stderr.includes("long.rs"));
  }
  assert.ok(
    failed,
    `expected script to fail on file > ${MAX_LINES} lines (${MAX_LINES + 1} lines)`,
  );

  // 5. 测试文件不计入规模，哪怕远超硬上限。
  fs.unlinkSync(longFile);
  fs.writeFileSync(
    validFile,
    'pub fn foo() {}\n#[cfg(test)]\n#[path = "valid_tests.rs"]\nmod tests;\n',
  );
  const testFile = path.join(backendDir, "valid_tests.rs");
  fs.writeFileSync(testFile, "#[test]\nfn t() {}\n".repeat(600));

  const testPassOutput = runScript();
  assert.ok(testPassOutput.includes("backend line limits passed"));
  assert.ok(testPassOutput.includes(`0 exceed ${MAX_LINES} lines`));

  console.log("backend line limits self-test passed");
} finally {
  fs.rmSync(fixtureRoot, { recursive: true, force: true });
}
