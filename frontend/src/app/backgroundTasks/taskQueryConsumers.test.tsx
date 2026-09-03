import { readFileSync } from "node:fs";
import { expect, it } from "vitest";

const names = [
  "ConversationSync",
  "ConversationDataMaintenance",
  "AiExecutionTask",
  "AgentLifecycleTask",
  "MemoryTask",
  "SkillBackup",
  "CatalogTask",
  "TeamTask",
  "TeamSession",
];

it.each(names)("%s 不再拥有自研请求运行时或poll interval", (name) => {
  const source = readFileSync(
    new URL(`./${name}Provider.tsx`, import.meta.url),
    "utf8",
  );
  expect(source).not.toContain(["useBackground", "TaskRuntime"].join(""));
  expect(source).not.toContain("setInterval(");
});
