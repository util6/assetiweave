#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import {
  isExplicitlyTestOnlyRustFile,
  maskRustNonCode,
  normalizeBackendRootAliases,
  walkRustFiles as walkAllRustFiles,
} from "./rust-test-files.mjs";

const projectRoot = path.resolve(process.argv[2] ?? ".");
const scriptRoot = path.dirname(new URL(import.meta.url).pathname);
const baselinePath = path.join(scriptRoot, "backend-layer-dependencies.baseline.json");
const strict = process.env.BOUNDARY_REQUIRE_ZERO_BACKEND_REVERSE_DEPS === "1";

const dependencyPairs = [
  { source: "store", target: "application", label: "Store → Application" },
  { source: "store", target: "infrastructure", label: "Store → Infrastructure" },
  { source: "infrastructure", target: "application", label: "Infrastructure → Application" },
];

function walkRustFiles(directory) {
  return walkAllRustFiles(directory).filter((file) => !isExplicitlyTestOnlyRustFile(file));
}

function stripTestOnlyItems(source) {
  const chars = source.split("");
  const attributePattern = /#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]/g;
  let attribute;

  while ((attribute = attributePattern.exec(source)) !== null) {
    let cursor = attribute.index + attribute[0].length;
    while (cursor < source.length && /\s/.test(source[cursor])) cursor += 1;
    while (source.startsWith("#[", cursor)) {
      const attributeEnd = source.indexOf("]", cursor + 2);
      if (attributeEnd < 0) break;
      cursor = attributeEnd + 1;
      while (cursor < source.length && /\s/.test(source[cursor])) cursor += 1;
    }

    const itemStart = attribute.index;
    let openBrace = -1;
    let semicolon = -1;
    for (let index = cursor; index < source.length; index += 1) {
      if (source[index] === "{") {
        openBrace = index;
        break;
      }
      if (source[index] === ";") {
        semicolon = index;
        break;
      }
    }

    let itemEnd = semicolon >= 0 ? semicolon + 1 : -1;
    if (openBrace >= 0) {
      let depth = 0;
      let inString = false;
      let escaped = false;
      for (let index = openBrace; index < source.length; index += 1) {
        const char = source[index];
        if (inString) {
          if (escaped) escaped = false;
          else if (char === "\\") escaped = true;
          else if (char === '"') inString = false;
          continue;
        }
        if (char === '"') {
          inString = true;
          continue;
        }
        if (char === "{") depth += 1;
        else if (char === "}") {
          depth -= 1;
          if (depth === 0) {
            itemEnd = index + 1;
            break;
          }
        }
      }
    }

    if (itemEnd < 0) continue;
    for (let index = itemStart; index < itemEnd; index += 1) {
      if (chars[index] !== "\n") chars[index] = " ";
    }
  }

  return chars.join("");
}

function stripCommentsAndStrings(source) {
  return source
    .replace(/b?r(#+)?"[\s\S]*?"\1/g, " ")
    .replace(/b?"(?:\\.|[^"\\])*"/g, " ")
    .replace(/b?'(?:\\.|[^'\\])'/g, " ")
    .replace(/\/\*[\s\S]*?\*\//g, " ")
    .replace(/\/\/[^\r\n]*/g, " ");
}

function normalizeGroupedPaths(source) {
  return normalizeBackendRootAliases(
    stripCommentsAndStrings(stripTestOnlyItems(maskRustNonCode(source))),
  )
    .replace(/[{}]/g, "::")
    .replace(/\s+/g, " ")
    .replace(/:{3,}/g, "::")
    .replace(/\s*::\s*/g, "::");
}

function countLayerReferences(root, sourceLayer, targetLayer) {
  const sourceRoot = path.join(root, "src-tauri", "src", "backend", sourceLayer);
  const targetPattern = new RegExp(`\\bbackend::${targetLayer}\\b`, "g");
  const references = [];

  for (const file of walkRustFiles(sourceRoot)) {
    const normalized = normalizeGroupedPaths(fs.readFileSync(file, "utf8"));
    const matches = [...normalized.matchAll(targetPattern)];
    if (matches.length > 0) {
      references.push({
        file: path.relative(root, file),
        count: matches.length,
      });
    }
  }
  return references;
}

const report = dependencyPairs.map((pair) => {
  const references = countLayerReferences(projectRoot, pair.source, pair.target);
  return { ...pair, references, count: references.reduce((sum, item) => sum + item.count, 0) };
});

let baseline;
try {
  baseline = JSON.parse(fs.readFileSync(baselinePath, "utf8"));
} catch (error) {
  console.error(`BOUNDARY CONFIGURATION ERROR: cannot read ${baselinePath}: ${error}`);
  process.exit(2);
}

console.log("Backend production reverse dependencies (lexical Rust module paths; only explicitly #[cfg(test)]-gated test/support modules excluded):");
let failed = false;
for (const item of report) {
  const ceiling = strict ? 0 : baseline.maxProductionReferences[item.sourceToTarget ?? `${item.source}To${item.target[0].toUpperCase()}${item.target.slice(1)}`];
  const status = item.count > ceiling ? "VIOLATION" : item.count > 0 ? "tracked debt" : "zero";
  console.log(`${item.label}: ${item.count} reference(s); ${strict ? "strict target" : `accepted ceiling ${ceiling}`} (${status})`);
  for (const reference of item.references) {
    console.log(`  ${reference.file}: ${reference.count}`);
  }
  if (item.count > ceiling) failed = true;
}

if (failed) process.exit(1);
