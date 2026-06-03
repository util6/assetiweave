import { readdirSync, readFileSync, statSync } from "node:fs";
import { relative, resolve } from "node:path";
import { describe, expect, it } from "vitest";

const srcRoot = resolve(new URL("..", import.meta.url).pathname);
const scannedRoots = ["components", "pages", "layouts"].map((dir) => resolve(srcRoot, dir));
const scannedExtensions = new Set([".ts", ".tsx"]);
const ignoredFilePatterns = [/\.test\./, /\.spec\./];

const forbiddenPatterns: Array<{ name: string; pattern: RegExp }> = [
  { name: "raw hex color", pattern: /#[0-9a-fA-F]{3,8}\b/g },
  { name: "fixed rgba color", pattern: /\brgba\(/g },
  {
    name: "native Tailwind color family",
    pattern:
      /\b(?:bg|text|border|from|via|to|ring|accent)-(?:white|black|slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose)(?:-\d{2,3}|\/\d+|\b)/g,
  },
];

describe("theme usage guard", () => {
  it("keeps business components on theme tokens and foundation recipes", () => {
    const violations = scanFiles().flatMap((filePath) => {
      const content = readFileSync(filePath, "utf8");
      return forbiddenPatterns.flatMap(({ name, pattern }) => findMatches(content, pattern, name, filePath));
    });

    expect(violations).toEqual([]);
  });
});

function scanFiles() {
  return scannedRoots.flatMap((root) => walk(root)).filter((filePath) => {
    if (ignoredFilePatterns.some((pattern) => pattern.test(filePath))) {
      return false;
    }

    return scannedExtensions.has(filePath.slice(filePath.lastIndexOf(".")));
  });
}

function walk(path: string): string[] {
  const stat = statSync(path);
  if (stat.isFile()) {
    return [path];
  }

  return readdirSync(path).flatMap((entry) => walk(resolve(path, entry)));
}

function findMatches(content: string, pattern: RegExp, name: string, filePath: string) {
  return [...content.matchAll(pattern)].map((match) => {
    const index = match.index ?? 0;
    const line = content.slice(0, index).split("\n").length;
    return `${relative(srcRoot, filePath)}:${line} ${name}: ${match[0]}`;
  });
}
