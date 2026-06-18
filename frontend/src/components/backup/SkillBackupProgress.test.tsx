import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { Translator } from "../../i18n/I18nProvider";
import type { SkillBackupTaskSnapshot } from "../../services/catalog";
import {
  isSkillBackupRunningFor,
  SkillBackupBackgroundTaskIndicator,
  SkillBackupInlineProgress,
} from "./SkillBackupProgress";

describe("SkillBackupProgress", () => {
  it("renders global and local progress for the active backup task", () => {
    const html = renderToStaticMarkup(
      <>
        <SkillBackupBackgroundTaskIndicator task={runningTask} t={translator} />
        <SkillBackupInlineProgress assetIds={["skill-b"]} task={runningTask} t={translator} />
      </>,
    );

    expect(html).toContain("已完成 1 / 3");
    expect(html).toContain("备份中 1/3");
    expect(html).toContain('aria-valuenow="1"');
  });

  it("only associates inline progress with assets in the running task", () => {
    expect(isSkillBackupRunningFor(runningTask, ["skill-b"])).toBe(true);
    expect(isSkillBackupRunningFor(runningTask, ["skill-z"])).toBe(false);
  });
});

const runningTask: SkillBackupTaskSnapshot = {
  id: "skill-backup-1",
  status: "running",
  asset_ids: ["skill-a", "skill-b", "skill-c"],
  total_count: 3,
  completed_count: 1,
  failed_count: 0,
  current_asset_id: "skill-b",
  started_at: "2026-06-18T00:00:00Z",
  finished_at: null,
  assets: [],
  errors: [],
  error: null,
};

const translator: Translator = (key, params) => {
  const messages: Record<string, string> = {
    "backup.action.running": "正在备份",
    "backup.action.runningCount": "备份中 {{completed}}/{{total}}",
    "backup.background.current": "当前：{{name}}",
    "backup.background.description": "已完成 {{completed}} / {{total}}",
    "backup.background.title": "正在后台备份",
  };
  return Object.entries(params ?? {}).reduce(
    (message, [name, value]) => message.replace(`{{${name}}}`, String(value)),
    messages[key] ?? key,
  );
};
