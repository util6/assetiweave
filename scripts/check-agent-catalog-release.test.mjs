import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import test from "node:test";

const ROOT = path.resolve(import.meta.dirname, "..");
const SCRIPT = path.join(ROOT, "scripts", "check-agent-catalog-release.mjs");
const CATALOG = path.join(ROOT, "builtin-assets", "agent-market", "catalog-v1.json");

function run(catalogPath, mode = "--static") {
  try {
    return { status: 0, stdout: execFileSync(process.execPath, [SCRIPT, mode, "--catalog", catalogPath], { encoding: "utf8" }) };
  } catch (error) {
    return { status: error.status ?? 1, stdout: `${error.stdout ?? ""}${error.stderr ?? ""}` };
  }
}

test("production catalog passes current-core and release gates", () => {
  const result = run(CATALOG, "--release");
  assert.equal(result.status, 0, result.stdout);
  assert.match(result.stdout, /catalog 2026\.08\.20\.2/);
});

test("release gate rejects fixture and placeholder catalog data", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "assetiweave-agent-catalog-"));
  const fixture = path.join(directory, "catalog.json");
  const catalog = JSON.parse(fs.readFileSync(CATALOG, "utf8"));
  catalog.items[0].upstream.homepage = "https://downloads.example.com/fixture";
  fs.writeFileSync(fixture, JSON.stringify(catalog));
  const result = run(fixture);
  assert.notEqual(result.status, 0);
  assert.match(result.stdout, /fixture|example\.com|placeholder/i);
  fs.rmSync(directory, { recursive: true, force: true });
});

test("release gate rejects a catalog that does not include the current core", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "assetiweave-agent-catalog-"));
  const fixture = path.join(directory, "catalog.json");
  const catalog = JSON.parse(fs.readFileSync(CATALOG, "utf8"));
  catalog.items[0].coreCompatibility = { min: "0.5.0", maxExclusive: "0.6.0" };
  fs.writeFileSync(fixture, JSON.stringify(catalog));
  const result = run(fixture);
  assert.notEqual(result.status, 0);
  assert.match(result.stdout, /does not include 0\.6\.1/);
  fs.rmSync(directory, { recursive: true, force: true });
});

test("release gate requires evidence to match the bundled catalog content", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "assetiweave-agent-catalog-"));
  const fixture = path.join(directory, "catalog.json");
  const catalog = JSON.parse(fs.readFileSync(CATALOG, "utf8"));
  catalog.items[0].distributions[0].sha256 = "1".repeat(64);
  fs.writeFileSync(fixture, JSON.stringify(catalog));
  const result = run(fixture, "--release");
  assert.notEqual(result.status, 0);
  assert.match(result.stdout, /catalogContentSha256|artifact SHA256/i);
  fs.rmSync(directory, { recursive: true, force: true });
});

test("release gate requires the published npm bin to match the catalog", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "assetiweave-agent-catalog-"));
  const fixture = path.join(directory, "catalog.json");
  const catalog = JSON.parse(fs.readFileSync(CATALOG, "utf8"));
  catalog.items.find((item) => item.id === "qoder").distributions[0].bin = "qoder";
  fs.writeFileSync(fixture, JSON.stringify(catalog));
  const result = run(fixture, "--release");
  assert.notEqual(result.status, 0);
  assert.match(result.stdout, /catalogContentSha256|package bin evidence/i);
  fs.rmSync(directory, { recursive: true, force: true });
});
