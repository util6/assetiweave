#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { isExplicitlyTestOnlyRustFile, walkRustFiles } from "./rust-test-files.mjs";

/**
 * 关注线：越过这条线不算违规，但应优先评估拆分，并在评审中说明理由。
 */
export const SOFT_LINE_LIMIT = 500;

/**
 * 硬上限：越过这条线必须先拆分出子业务模块，门禁直接失败。
 */
export const MAX_LINES = 1000;

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
  const candidateDirs = [
    path.resolve(root, "backend/asset_core/src"),
    path.resolve(root, "backend/agent_engine/src"),
  ].filter((d) => fs.existsSync(d));

  const legacyDir = path.resolve(root, "src-tauri/src/backend");
  if (candidateDirs.length === 0 && fs.existsSync(legacyDir)) {
    candidateDirs.push(legacyDir);
  }

  if (candidateDirs.length === 0) {
    throw new Error(`backend directory not found under: ${root}`);
  }

  const files = candidateDirs.flatMap((dir) => walkRustFiles(dir));
  const overLimit = [];
  const overSoftLimit = [];
  let productionCount = 0;

  for (const file of files) {
    if (isExplicitlyTestOnlyRustFile(file)) continue;
    const relPath = path.relative(root, file).replace(/\\/g, "/");
    productionCount += 1;
    const content = fs.readFileSync(file, "utf8");
    const lineCount = countLines(content);
    if (lineCount > MAX_LINES) {
      overLimit.push({ file: relPath, lineCount });
    } else if (lineCount > SOFT_LINE_LIMIT) {
      overSoftLimit.push({ file: relPath, lineCount });
    }
  }

  return { productionCount, overLimit, overSoftLimit };
}

const isDirectExecution =
  process.argv[1] &&
  fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);

if (isDirectExecution) {
  const root = process.argv[2] ?? process.cwd();
  try {
    const { productionCount, overLimit, overSoftLimit } =
      checkBackendLineLimits(root);

    if (overSoftLimit.length > 0) {
      console.log(
        `backend line limits notice: ${overSoftLimit.length} production file(s) between ` +
          `${SOFT_LINE_LIMIT} and ${MAX_LINES} lines, consider splitting:`,
      );
      for (const item of overSoftLimit) {
        console.log(`  ${item.lineCount} lines: ${item.file}`);
      }
    }

    if (overLimit.length > 0) {
      console.error(
        `backend line limits failed: ${overLimit.length} production file(s) exceed ` +
          `${MAX_LINES} lines:`,
      );
      for (const item of overLimit) {
        console.error(`  ${item.lineCount} lines: ${item.file}`);
      }
      process.exit(1);
    }
    console.log(
      `backend line limits passed: ${productionCount} production files, 0 exceed ` +
        `${MAX_LINES} lines`,
    );
  } catch (err) {
    console.error(err.message);
    process.exit(2);
  }
}
