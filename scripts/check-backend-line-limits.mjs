#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { isExplicitlyTestOnlyRustFile, walkRustFiles } from "./rust-test-files.mjs";

export const MAX_LINES = 500;

export function countLines(content) {
  if (content.length === 0) return 0;
  const normalized = content.endsWith("\r\n")
    ? content.slice(0, -2)
    : content.endsWith("\n")
      ? content.slice(0, -1)
      : content;
  return normalized.length === 0 ? 0 : normalized.split(/\r?\n/).length;
}

export function checkBackendLineLimits(root = process.cwd()) {
  const backendDir = path.resolve(root, "src-tauri/src/backend");

  if (!fs.existsSync(backendDir)) {
    throw new Error(`backend directory not found: ${backendDir}`);
  }

  const files = walkRustFiles(backendDir);
  const overLimit = [];
  let productionCount = 0;

  for (const file of files) {
    if (isExplicitlyTestOnlyRustFile(file)) continue;
    productionCount += 1;
    const content = fs.readFileSync(file, "utf8");
    const lineCount = countLines(content);
    if (lineCount > MAX_LINES) {
      overLimit.push({
        file: path.relative(root, file),
        lineCount,
      });
    }
  }

  return {
    productionCount,
    overLimit,
  };
}

const isDirectExecution =
  process.argv[1] &&
  fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);

if (isDirectExecution) {
  const root = process.argv[2] ?? process.cwd();
  try {
    const { productionCount, overLimit } = checkBackendLineLimits(root);
    if (overLimit.length > 0) {
      console.error(
        `backend line limits failed: ${overLimit.length} production file(s) exceed ${MAX_LINES} lines:`,
      );
      for (const item of overLimit) {
        console.error(`  ${item.lineCount} lines: ${item.file}`);
      }
      process.exit(1);
    }
    console.log(
      `backend line limits passed: ${productionCount} production files, 0 exceed ${MAX_LINES} lines`,
    );
  } catch (err) {
    console.error(err.message);
    process.exit(2);
  }
}
