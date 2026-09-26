#!/usr/bin/env node

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { countLines, MAX_LINES } from "./check-backend-line-limits.mjs";

// Unit test countLines directly
assert.equal(countLines(""), 0);
assert.equal(countLines("pub fn foo() {}\n".repeat(500)), 500);
assert.equal(countLines("pub fn foo() {}\r\n".repeat(500)), 500);
assert.equal(countLines("pub fn foo() {}\n".repeat(500).slice(0, -1)), 500);
assert.equal(countLines("pub fn foo() {}\n".repeat(501)), 501);

const scriptPath = path.resolve("scripts/check-backend-line-limits.mjs");
const fixtureRoot = fs.mkdtempSync(path.join(os.tmpdir(), "assetiweave-line-guard-"));

try {
  const backendDir = path.join(fixtureRoot, "src-tauri/src/backend/domain");
  fs.mkdirSync(backendDir, { recursive: true });

  // 1. Valid file: exactly 500 lines with trailing newline - should pass
  const validFile = path.join(backendDir, "valid.rs");
  fs.writeFileSync(validFile, "pub fn foo() {}\n".repeat(500));

  const passOutput = execFileSync(process.execPath, [scriptPath, fixtureRoot], {
    encoding: "utf8",
  });
  assert.ok(passOutput.includes("backend line limits passed"));
  assert.ok(passOutput.includes("0 exceed 500 lines"));

  // 2. Over limit file: exactly 501 lines with trailing newline - should fail
  const longFile = path.join(backendDir, "long.rs");
  fs.writeFileSync(longFile, "pub fn bar() {}\n".repeat(501));

  let failed = false;
  try {
    execFileSync(process.execPath, [scriptPath, fixtureRoot], {
      encoding: "utf8",
      stdio: "pipe",
    });
  } catch (err) {
    failed = true;
    const stderr = err.stderr?.toString() ?? "";
    assert.ok(stderr.includes("1 production file(s) exceed 500 lines"));
    assert.ok(stderr.includes("501 lines"));
    assert.ok(stderr.includes("long.rs"));
  }
  assert.ok(failed, "expected script to fail on file > 500 lines (501 lines)");

  // 3. Test-only file should not be counted even if > 500 lines
  fs.unlinkSync(longFile);
  fs.writeFileSync(
    validFile,
    'pub fn foo() {}\n#[cfg(test)]\n#[path = "valid_tests.rs"]\nmod tests;\n',
  );
  const testFile = path.join(backendDir, "valid_tests.rs");
  fs.writeFileSync(testFile, "#[test]\nfn t() {}\n".repeat(600));

  const testPassOutput = execFileSync(process.execPath, [scriptPath, fixtureRoot], {
    encoding: "utf8",
  });
  assert.ok(testPassOutput.includes("backend line limits passed"));
  assert.ok(testPassOutput.includes("0 exceed 500 lines"));

  console.log("backend line limits self-test passed");
} finally {
  fs.rmSync(fixtureRoot, { recursive: true, force: true });
}
