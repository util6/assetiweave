#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { maskRustNonCode } from "./rust-test-files.mjs";

const projectRoot = path.resolve(process.argv[2] ?? ".");
const backendRoot = path.join(projectRoot, "src-tauri", "src", "backend");
const moduleFile = path.join(backendRoot, "mod.rs");
const requiredProductionModules = new Set([
  "application",
  "domain",
  "infrastructure",
  "store",
]);

function fail(message) {
  console.error(`BACKEND ROOT LAYOUT VIOLATION: ${message}`);
  process.exitCode = 1;
}

if (!fs.existsSync(moduleFile) || !fs.statSync(moduleFile).isFile()) {
  console.error(`BACKEND ROOT LAYOUT CONFIGURATION ERROR: missing ${moduleFile}`);
  process.exit(2);
}

const source = fs.readFileSync(moduleFile, "utf8");
const code = maskRustNonCode(source);
const declarationPattern = /(^|\n)[\t ]*(?<attributes>(?:#\s*\[[^\]\n]*\][\t ]*\n[\t ]*)*)(?<visibility>pub(?:\s*\([^)]*\))?\s+)?mod\s+(?<name>[A-Za-z_]\w*)\b[^;{}]*(?<terminator>[;{])/g;
const declarations = [...code.matchAll(declarationPattern)].map((match) => ({
  name: match.groups.name,
  attributes: match.groups.attributes,
  visibility: match.groups.visibility,
  terminator: match.groups.terminator,
}));
const declarationCounts = new Map();
for (const declaration of declarations) {
  declarationCounts.set(
    declaration.name,
    (declarationCounts.get(declaration.name) ?? 0) + 1,
  );
}

for (const moduleName of requiredProductionModules) {
  if (declarationCounts.get(moduleName) !== 1) {
    fail(`backend/mod.rs must declare production module ${moduleName} exactly once`);
    continue;
  }
  const declaration = declarations.find((item) => item.name === moduleName);
  if (/\btest\b/.test(declaration.attributes)) {
    fail(`production module ${moduleName} must not be gated by #[cfg(test)]`);
  }
}

const testSupport = declarations.filter((item) => item.name === "test_support");
if (testSupport.length > 1) {
  fail("backend/mod.rs may declare test_support at most once");
} else if (testSupport.length === 1 && !/cfg\s*\(\s*test\s*\)/.test(testSupport[0].attributes)) {
  fail("test_support is allowed only when its module declaration has #[cfg(test)]");
}

const allowedNames = new Set(requiredProductionModules);
if (testSupport.length === 1) allowedNames.add("test_support");
for (const [moduleName, count] of declarationCounts) {
  if (!allowedNames.has(moduleName)) {
    fail(`unexpected backend root module declaration ${moduleName} (${count} occurrence(s))`);
  }
}

if (/\bpub\s*(?:\([^)]*\)\s*)?use\b/.test(code)) {
  fail("backend/mod.rs must not contain re-exports; import from the owning layer");
}

let directoryNames;
try {
  directoryNames = fs
    .readdirSync(backendRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();
} catch (error) {
  console.error(`BACKEND ROOT LAYOUT CONFIGURATION ERROR: cannot inspect ${backendRoot}: ${error}`);
  process.exit(2);
}
const expectedDirectories = [...requiredProductionModules];
if (allowedNames.has("test_support") && directoryNames.includes("test_support")) {
  expectedDirectories.push("test_support");
}
expectedDirectories.sort();
if (JSON.stringify(directoryNames) !== JSON.stringify(expectedDirectories)) {
  fail(
    `backend/ top-level directories must be exactly [${expectedDirectories.join(", ")}], found [${directoryNames.join(", ")}]`,
  );
}

if (process.exitCode !== 1) {
  console.log(
    `backend root layout passed: ${[...requiredProductionModules].join(", ")}${testSupport.length ? ", test_support (cfg(test))" : ""}`,
  );
}
