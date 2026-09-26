import fs from "node:fs";
import path from "node:path";

function blankNonNewlineRange(chars, start, end) {
  for (let index = start; index < end; index += 1) {
    if (chars[index] !== "\n" && chars[index] !== "\r") chars[index] = " ";
  }
}

export function maskRustNonCode(source) {
  const chars = source.split("");
  let index = 0;

  while (index < source.length) {
    if (source.startsWith("//", index)) {
      const start = index;
      while (index < source.length && source[index] !== "\n") index += 1;
      blankNonNewlineRange(chars, start, index);
      continue;
    }
    if (source.startsWith("/*", index)) {
      const start = index;
      let depth = 1;
      index += 2;
      while (index < source.length && depth > 0) {
        if (source.startsWith("/*", index)) {
          depth += 1;
          index += 2;
        } else if (source.startsWith("*/", index)) {
          depth -= 1;
          index += 2;
        } else index += 1;
      }
      blankNonNewlineRange(chars, start, index);
      continue;
    }

    let rawStringStart = index;
    if (source.startsWith("br", index)) rawStringStart += 2;
    else if (source[index] === "r") rawStringStart += 1;
    else rawStringStart = -1;
    if (rawStringStart >= 0) {
      let quoteIndex = rawStringStart;
      while (source[quoteIndex] === "#") quoteIndex += 1;
      const hashes = quoteIndex - rawStringStart;
      if (source[quoteIndex] === '"') {
        const start = index;
        index = quoteIndex + 1;
        const terminator = `"${"#".repeat(hashes)}`;
        const closeIndex = source.indexOf(terminator, index);
        index = closeIndex >= 0 ? closeIndex + terminator.length : source.length;
        blankNonNewlineRange(chars, start, index);
        continue;
      }
    }

    const stringStart = source[index] === '"' ? index : source[index] === "b" && source[index + 1] === '"' ? index + 1 : -1;
    if (stringStart >= 0) {
      const start = index;
      index = stringStart + 1;
      let escaped = false;
      while (index < source.length) {
        const char = source[index];
        index += 1;
        if (escaped) escaped = false;
        else if (char === "\\") escaped = true;
        else if (char === '"') break;
      }
      blankNonNewlineRange(chars, start, index);
      continue;
    }

    if (source[index] === "'" && /^'(?:\\.|[^'\\])'/.test(source.slice(index))) {
      const literal = source.slice(index).match(/^'(?:\\.|[^'\\])'/)[0];
      blankNonNewlineRange(chars, index, index + literal.length);
      index += literal.length;
      continue;
    }
    index += 1;
  }

  return chars.join("");
}

export function stripTestOnlyItems(source) {
  const sanitized = maskRustNonCode(source);
  const chars = sanitized.split("");
  const attributePattern = /#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]/g;
  let attribute;

  while ((attribute = attributePattern.exec(sanitized)) !== null) {
    let cursor = attribute.index + attribute[0].length;
    while (cursor < sanitized.length && /\s/.test(sanitized[cursor])) cursor += 1;
    while (sanitized.startsWith("#[", cursor)) {
      const attributeEnd = sanitized.indexOf("]", cursor + 2);
      if (attributeEnd < 0) break;
      cursor = attributeEnd + 1;
      while (cursor < sanitized.length && /\s/.test(sanitized[cursor])) cursor += 1;
    }

    let openBrace = -1;
    let semicolon = -1;
    for (let itemIndex = cursor; itemIndex < sanitized.length; itemIndex += 1) {
      if (sanitized[itemIndex] === "{") {
        openBrace = itemIndex;
        break;
      }
      if (sanitized[itemIndex] === ";") {
        semicolon = itemIndex;
        break;
      }
    }

    let itemEnd = semicolon >= 0 ? semicolon + 1 : -1;
    if (openBrace >= 0) {
      let depth = 0;
      for (let itemIndex = openBrace; itemIndex < sanitized.length; itemIndex += 1) {
        if (sanitized[itemIndex] === "{") depth += 1;
        else if (sanitized[itemIndex] === "}") {
          depth -= 1;
          if (depth === 0) {
            itemEnd = itemIndex + 1;
            break;
          }
        }
      }
    }

    if (itemEnd < 0) continue;
    blankNonNewlineRange(chars, attribute.index, itemEnd);
  }

  return chars.join("");
}

export function normalizeBackendRootAliases(source) {
  const code = maskRustNonCode(source);
  const aliases = new Set();
  const directAlias = /\buse\s+(?:crate|self)\s*::\s*backend\s+as\s+([A-Za-z_][A-Za-z0-9_]*)\s*;/g;
  const groupedAlias = /\buse\s+(?:crate|self)\s*::\s*\{([\s\S]*?)\}\s*;/g;
  let match;

  while ((match = directAlias.exec(code)) !== null) aliases.add(match[1]);
  while ((match = groupedAlias.exec(code)) !== null) {
    const importedNames = /(?:^|,)\s*backend\s+as\s+([A-Za-z_][A-Za-z0-9_]*)/g;
    let importedName;
    while ((importedName = importedNames.exec(match[1])) !== null) aliases.add(importedName[1]);
  }

  let normalized = code;
  for (const alias of aliases) {
    normalized = normalized.replace(new RegExp(`\\b${alias}\\s*::`, "g"), "backend::");
  }
  return normalized;
}

function moduleSourceCandidates(sourceFile, moduleName) {
  const directory = path.dirname(sourceFile);
  const sourceName = path.basename(sourceFile);
  const moduleDirectory = sourceName === "mod.rs" || sourceName === "lib.rs" || sourceName === "main.rs"
    ? directory
    : path.join(directory, path.basename(sourceFile, ".rs"));
  return [
    path.join(directory, `${moduleName}.rs`),
    path.join(directory, moduleName, "mod.rs"),
    path.join(moduleDirectory, `${moduleName}.rs`),
    path.join(moduleDirectory, moduleName, "mod.rs"),
  ].map((candidate) => path.resolve(candidate));
}

function hasCfgTestModuleLink(sourceFile, targetFile) {
  if (!fs.existsSync(sourceFile)) return false;
  const rawSource = fs.readFileSync(sourceFile, "utf8");
  const source = maskRustNonCode(rawSource);
  const declarations = /((?:\s*#\s*\[[^\]]*\]\s*)*)(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;/g;
  let declaration;

  while ((declaration = declarations.exec(source)) !== null) {
    const attributes = declaration[1];
    const moduleName = declaration[2];
    const originalAttributes = rawSource.slice(
      declaration.index,
      declaration.index + attributes.length,
    );
    const pathAttribute = originalAttributes.match(/#\s*\[\s*path\s*=\s*"([^"]+)"\s*\]/);
    const candidates = pathAttribute
      ? [path.resolve(path.dirname(sourceFile), pathAttribute[1])]
      : moduleSourceCandidates(sourceFile, moduleName);
    if (!candidates.includes(path.resolve(targetFile))) continue;
    if (/#[\s]*\[[\s]*cfg\s*\([\s]*test[\s]*\)[\s]*\]/.test(attributes)) return true;
  }

  return false;
}

/**
 * Treat conventional Rust test/support files as test-only only when a parent
 * module explicitly includes that exact file behind #[cfg(test)]. A matching
 * filename on its own is never enough to remove code from production checks.
 */
export function isExplicitlyTestOnlyRustFile(filePath) {
  const absolutePath = path.resolve(filePath);
  const fileName = path.basename(absolutePath);
  const conventionalTestFile = fileName.endsWith("_tests.rs") || fileName === "tests.rs" || fileName === "test_support.rs";
  if (!conventionalTestFile) return false;

  const directory = path.dirname(absolutePath);
  const siblingModuleName = fileName.endsWith("_tests.rs")
    ? fileName.slice(0, -"_tests.rs".length)
    : path.basename(fileName, ".rs");
  const parentModuleCandidates = new Set([
    path.join(directory, `${siblingModuleName}.rs`),
    path.join(directory, "mod.rs"),
    path.join(directory, siblingModuleName, "mod.rs"),
    path.join(path.dirname(directory), `${path.basename(directory)}.rs`),
  ]);

  return [...parentModuleCandidates].some((parent) => hasCfgTestModuleLink(parent, absolutePath));
}

export function walkRustFiles(directory) {
  if (!fs.existsSync(directory)) return [];
  const files = [];
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...walkRustFiles(entryPath));
    else if (entry.isFile() && entry.name.endsWith(".rs")) files.push(entryPath);
  }
  return files;
}
