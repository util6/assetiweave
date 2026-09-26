#!/usr/bin/env node

import fs from "node:fs";
import process from "node:process";
import {
  isExplicitlyTestOnlyRustFile,
  maskRustNonCode,
  normalizeBackendRootAliases,
  stripTestOnlyItems,
  walkRustFiles,
} from "./rust-test-files.mjs";

const [scope, pattern] = process.argv.slice(2);
if (!scope || !pattern) {
  console.error("usage: check-rust-production-pattern.mjs <scope> <regex|--flag>");
  process.exit(2);
}

let found = false;

function reportMatch(file, source, index, message) {
  const lineNumber = source.slice(0, index).split("\n").length;
  const lineText = source.split(/\r?\n/)[lineNumber - 1]?.trim() ?? "";
  console.log(`${file}:${lineNumber}:${lineText}${message ? ` (${message})` : ""}`);
  found = true;
}

function splitTopLevelEntries(content) {
  const entries = [];
  let depth = 0;
  let current = "";
  for (let i = 0; i < content.length; i++) {
    const ch = content[i];
    if (ch === "{" || ch === "(") depth++;
    else if (ch === "}" || ch === ")") depth--;
    else if (ch === "," && depth === 0) {
      if (current.trim()) entries.push(current.trim());
      current = "";
      continue;
    }
    current += ch;
  }
  if (current.trim()) entries.push(current.trim());
  return entries;
}

function checkDomainPurity(file, source) {
  // 1. Forbidden framework / other layers / runtime IO in Domain
  const forbiddenDeps = /\b(?:backend::(?:application|store|infrastructure|dto)|sqlx|tauri|std::fs|tokio::fs|std::process)\b/g;
  let match;
  while ((match = forbiddenDeps.exec(source)) !== null) {
    reportMatch(file, source, match.index, `forbidden domain dependency '${match[0]}'`);
  }

  // 2. Direct dynamic environment calls
  const directEnv = /\bstd::env::(?:var|var_os|vars|vars_os|args|args_os|current_dir|temp_dir|set_var|remove_var|current_exe)\b/g;
  while ((match = directEnv.exec(source)) !== null) {
    reportMatch(file, source, match.index, `forbidden dynamic env call '${match[0]}'`);
  }

  // 3. Domain importing std::env (direct, aliased, or grouped) and dynamically reading env
  const envImport = /\buse\s+(?:(?:::)?std::env(?:\s+as\s+([A-Za-z_][A-Za-z0-9_]*))?|(?:::)?std::\{[^;}]*\benv(?:\s+as\s+([A-Za-z_][A-Za-z0-9_]*))?[^;}]*\})\s*;/g;
  while ((match = envImport.exec(source)) !== null) {
    const alias = match[1] || match[2] || "env";
    const dynamicCall = new RegExp(`\\b${alias}::(?:var|var_os|vars|vars_os|args|args_os|current_dir|temp_dir|set_var|remove_var|current_exe)\\b`, "g");
    let callMatch;
    while ((callMatch = dynamicCall.exec(source)) !== null) {
      reportMatch(file, source, callMatch.index, `forbidden dynamic env call '${callMatch[0]}'`);
    }
  }
}

function checkCrossLayerGlob(file, source) {
  // Matches any `use` statement in production code importing from domain, store, or infrastructure
  // that contains a glob `*` on that layer (e.g. `::*`, `{self, *}`, `{foo, *}`, `{*}`)
  const useStmt = /\buse\s+(?:crate::)?backend::([^;]+);/g;
  let match;
  while ((match = useStmt.exec(source)) !== null) {
    const body = match[1].trim();
    if (body.startsWith("{") && body.endsWith("}")) {
      const inner = body.slice(1, -1);
      const entries = splitTopLevelEntries(inner);
      for (const entry of entries) {
        if (/^\b(?:domain|store|infrastructure)\b/.test(entry) && entry.includes("*")) {
          reportMatch(file, source, match.index, `forbidden cross-layer glob import '${entry.replace(/\s+/g, " ")}'`);
        }
      }
    } else {
      if (/^\b(?:domain|store|infrastructure)\b/.test(body) && body.includes("*")) {
        reportMatch(file, source, match.index, `forbidden cross-layer glob import '${body.replace(/\s+/g, " ")}'`);
      }
    }
  }
}

function checkInfraBridge(file, source) {
  // Infrastructure must not re-export Domain or Store symbols via pub(crate) use or pub use
  const pubUseStmt = /\bpub(?:\s*\(\s*crate\s*\))?\s+use\s+(?:crate::)?backend::([^;]+);/g;
  let match;
  while ((match = pubUseStmt.exec(source)) !== null) {
    const body = match[1].trim();
    if (body.startsWith("{") && body.endsWith("}")) {
      const inner = body.slice(1, -1);
      const entries = splitTopLevelEntries(inner);
      for (const entry of entries) {
        if (/^\b(?:domain|store)\b/.test(entry)) {
          reportMatch(file, source, match.index, `forbidden infrastructure pub(crate) use bridging Domain/Store '${entry.replace(/\s+/g, " ")}'`);
        }
      }
    } else {
      if (/^\b(?:domain|store)\b/.test(body)) {
        reportMatch(file, source, match.index, `forbidden infrastructure pub(crate) use bridging Domain/Store '${body.replace(/\s+/g, " ")}'`);
      }
    }
  }

  // Also reject declaring bridge modules like `mod cards;` or `pub mod cards;` in conversations/mod.rs
  if (file.endsWith("infrastructure/conversations/mod.rs") || file.endsWith("infrastructure\\conversations\\mod.rs")) {
    const bridgeMod = /\b(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+(?:cards|pricing|usage_repo)\s*;/g;
    while ((match = bridgeMod.exec(source)) !== null) {
      reportMatch(file, source, match.index, `forbidden conversations bridge module '${match[0]}'`);
    }
  }
}

function checkGenericPattern(file, source, patternStr) {
  const flags = patternStr.includes("^") || patternStr.includes("$") ? "gm" : "g";
  const forbidden = new RegExp(patternStr, flags);
  let match;
  while ((match = forbidden.exec(source)) !== null) {
    reportMatch(file, source, match.index);
    if (match[0].length === 0) forbidden.lastIndex += 1;
  }
}

const scopedFiles = !fs.existsSync(scope)
  ? []
  : fs.statSync(scope).isFile()
    ? [scope]
    : walkRustFiles(scope);

for (const file of scopedFiles) {
  if (isExplicitlyTestOnlyRustFile(file)) continue;
  const source = normalizeBackendRootAliases(
    stripTestOnlyItems(maskRustNonCode(fs.readFileSync(file, "utf8"))),
  );

  if (pattern === "--domain-purity") {
    checkDomainPurity(file, source);
  } else if (pattern === "--cross-layer-glob") {
    checkCrossLayerGlob(file, source);
  } else if (pattern === "--infra-bridge") {
    checkInfraBridge(file, source);
  } else {
    checkGenericPattern(file, source, pattern);
  }
}

if (found) process.exit(1);
