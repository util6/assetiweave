#!/usr/bin/env node

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import {
  isExplicitlyTestOnlyRustFile,
  normalizeBackendRootAliases,
  stripTestOnlyItems,
  walkRustFiles as walkAllRustFiles,
} from "./rust-test-files.mjs";

const root = path.resolve(process.argv[2] ?? process.cwd());
const backendRoot = path.join(root, "src-tauri/src/backend");
const rustRoot = path.join(root, "src-tauri/src");
const legacyModules = [
  "agent_market",
  "agents",
  "ai_execution",
  "conversations",
  "dto",
  "error",
  "executor",
  "planner",
  "projection",
  "scanner",
  "search",
];

function walkRustFiles(directory) {
  return walkAllRustFiles(directory).sort();
}

function isProductionRustFile(filePath) {
  return filePath.endsWith(".rs") && !isExplicitlyTestOnlyRustFile(filePath);
}

function findClosingBrace(source, openingBrace) {
  let depth = 0;
  for (let index = openingBrace; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}") {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  return -1;
}

function splitTopLevelImports(importList) {
  const imports = [];
  let depth = 0;
  let start = 0;
  for (let index = 0; index < importList.length; index += 1) {
    if (importList[index] === "{") depth += 1;
    if (importList[index] === "}") depth -= 1;
    if (importList[index] === "," && depth === 0) {
      imports.push(importList.slice(start, index));
      start = index + 1;
    }
  }
  imports.push(importList.slice(start));
  return imports;
}

function countModulePaths(contents, module) {
  const direct = new RegExp(`backend\\s*::\\s*${module}(?=\\s*(?:::|\\bas\\b|[,};]))`, "g");
  let count = [...contents.matchAll(direct)].length;
  const backendGroups = /backend\s*::\s*\{/g;
  for (const group of contents.matchAll(backendGroups)) {
    const openingBrace = group.index + group[0].lastIndexOf("{");
    const closingBrace = findClosingBrace(contents, openingBrace);
    if (closingBrace < 0) continue;
    for (const importedPath of splitTopLevelImports(
      contents.slice(openingBrace + 1, closingBrace),
    )) {
      const firstSegment = importedPath.match(/^\s*([A-Za-z_][A-Za-z0-9_]*)/);
      if (firstSegment?.[1] === module) count += 1;
    }
  }
  return count;
}

function measureWorktree(module) {
  const moduleDirectory = path.join(backendRoot, module);
  const moduleFiles = walkRustFiles(moduleDirectory).filter(isProductionRustFile);
  const sourceFiles = walkRustFiles(rustRoot).filter(isProductionRustFile);
  let callerFiles = 0;
  let references = 0;

  for (const filePath of sourceFiles) {
    if (filePath === moduleDirectory || filePath.startsWith(`${moduleDirectory}${path.sep}`)) {
      continue;
    }
    const contents = normalizeBackendRootAliases(stripTestOnlyItems(readFileSync(filePath, "utf8")));
    const referencesInFile = countModulePaths(contents, module);
    if (referencesInFile > 0) {
      callerFiles += 1;
      references += referencesInFile;
    }
  }

  return {
    files: moduleFiles.length,
    lines: moduleFiles.reduce(
      (total, filePath) => total + readFileSync(filePath, "utf8").split("\n").length - 1,
      0,
    ),
    callerFiles,
    references,
  };
}

const captureIndex = process.argv.indexOf("--capture");
const baselineIndex = process.argv.indexOf("--baseline");
const defaultBaseline = path.join(root, "scripts/backend-legacy-modules.baseline.json");
const baselinePath =
  (baselineIndex >= 0 && process.argv[baselineIndex + 1]) ||
  process.env.BOUNDARY_LEGACY_BASELINE ||
  defaultBaseline;
const requireZero = process.env.BOUNDARY_REQUIRE_NO_LEGACY === "1";
const retiredModules = new Set(
  (process.env.BOUNDARY_RETIRED_MODULES ?? "")
    .split(",")
    .map((module) => module.trim())
    .filter(Boolean),
);

const currentByModule = Object.fromEntries(
  legacyModules.map((module) => [module, measureWorktree(module)]),
);

if (captureIndex >= 0) {
  const outputPath = process.argv[captureIndex + 1] || baselinePath;
  mkdirSync(path.dirname(path.resolve(outputPath)), { recursive: true });
  writeFileSync(
    outputPath,
    `${JSON.stringify(
      {
        schemaVersion: 1,
        metricVersion: "backend-root-module-paths-v2",
        description: "Accepted dirty-worktree baseline for monotonic legacy-module retirement; refresh only by explicit architecture-plan change.",
        modules: currentByModule,
      },
      null,
      2,
    )}\n`,
  );
  console.log(`captured legacy module baseline: ${outputPath}`);
  process.exit(0);
}

let baseline = null;
if (existsSync(baselinePath)) {
  try {
    const saved = JSON.parse(readFileSync(baselinePath, "utf8"));
    if (saved.metricVersion !== "backend-root-module-paths-v2") {
      throw new Error(`unsupported metric version: ${saved.metricVersion ?? "missing"}`);
    }
    baseline = saved.modules;
  } catch (error) {
    console.error(`BOUNDARY CONFIGURATION ERROR: could not read ${baselinePath}: ${error.message}`);
    process.exit(1);
  }
}

let failure = false;
let totalPresent = 0;
let totalFiles = 0;
let totalLines = 0;
let totalCallerFiles = 0;
let totalReferences = 0;

console.log("Legacy backend modules (only explicitly #[cfg(test)]-gated test/support modules excluded; caller/reference counts are lexical backend module paths, not AST analysis):");
console.log("module             dirs  files  lines  caller-files  references  Δfiles  Δlines  Δcallers  Δrefs");

for (const module of legacyModules) {
  const current = currentByModule[module];
  const saved = baseline?.[module];
  const present = existsSync(path.join(backendRoot, module)) ? 1 : 0;
  totalPresent += present;
  totalFiles += current.files;
  totalLines += current.lines;
  totalCallerFiles += current.callerFiles;
  totalReferences += current.references;

  const deltas = saved
    ? ["files", "lines", "callerFiles", "references"].map((key) => current[key] - saved[key])
    : ["n/a", "n/a", "n/a", "n/a"];
  console.log(
    `${module.padEnd(18)} ${String(present).padStart(4)} ${String(current.files).padStart(6)} ${String(current.lines).padStart(6)} ${String(current.callerFiles).padStart(13)} ${String(current.references).padStart(11)} ${String(deltas[0]).padStart(7)} ${String(deltas[1]).padStart(7)} ${String(deltas[2]).padStart(9)} ${String(deltas[3]).padStart(7)}`,
  );

  if (saved) {
    for (const key of ["files", "lines", "callerFiles", "references"]) {
      if (current[key] > saved[key]) {
        console.error(
          `BOUNDARY VIOLATION: legacy module ${module} ${key} grew from accepted worktree baseline ${saved[key]} to ${current[key]}`,
        );
        failure = true;
      }
    }
  }
  if (requireZero && present) {
    console.error(`BOUNDARY VIOLATION: legacy backend module remains: ${module}`);
    failure = true;
  }
  if (retiredModules.has(module) && (present || current.references > 0)) {
    console.error(
      `BOUNDARY VIOLATION: retired backend module ${module} remains (directory=${present}, production references=${current.references})`,
    );
    failure = true;
  }
}

console.log(
  `total              ${String(totalPresent).padStart(4)} ${String(totalFiles).padStart(6)} ${String(totalLines).padStart(6)} ${String(totalCallerFiles).padStart(13)} ${String(totalReferences).padStart(11)}`,
);
console.log(`old top-level directories remaining: ${totalPresent}/${legacyModules.length}`);

if (failure) process.exit(1);
