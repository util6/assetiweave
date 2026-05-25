import { invoke } from "@tauri-apps/api/core";
import { navigationModel } from "../navigation/menu";
import type { NavigationModel } from "../navigation/types";
import type { AppOverview, Asset, DeploymentPlan, ExecutionResult, Source, TargetProfile } from "../types";

const fallbackAssets: Asset[] = [
  {
    id: "demo-algorithmic-art",
    source_id: "local-skills",
    name: "algorithmic-art",
    kind: "skill",
    format: "directory",
    relative_path: "skills/algorithmic-art/SKILL.md",
    absolute_path: "/assets/skills/algorithmic-art/SKILL.md",
    entry_file: "SKILL.md",
    description:
      "Creating algorithmic art using p5.js with seeded randomness and interactive parameter exploration.",
    discovered_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  },
  {
    id: "demo-android-native-dev",
    source_id: "local-skills",
    name: "android-native-dev",
    kind: "skill",
    format: "directory",
    relative_path: "skills/android-native-dev/SKILL.md",
    absolute_path: "/assets/skills/android-native-dev/SKILL.md",
    entry_file: "SKILL.md",
    description:
      "Android native application development and UI design guide covering Material Design 3 and Kotlin/Compose.",
    discovered_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  },
  {
    id: "demo-brand-guidelines",
    source_id: "local-skills",
    name: "brand-guidelines",
    kind: "rule",
    format: "markdown",
    relative_path: "rules/brand-guidelines.md",
    absolute_path: "/assets/rules/brand-guidelines.md",
    description:
      "Applies official brand colors, typography, and presentation guidance to generated artifacts.",
    discovered_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  },
  {
    id: "demo-browser-harness",
    source_id: "local-skills",
    name: "browser-harness",
    kind: "agent",
    format: "script",
    relative_path: "agents/browser-harness.ts",
    absolute_path: "/assets/agents/browser-harness.ts",
    description:
      "Direct browser control for automation, scraping, testing, and local web page interaction.",
    discovered_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  },
  {
    id: "demo-canvas-design",
    source_id: "local-skills",
    name: "canvas-design",
    kind: "skill",
    format: "directory",
    relative_path: "skills/canvas-design/SKILL.md",
    absolute_path: "/assets/skills/canvas-design/SKILL.md",
    entry_file: "SKILL.md",
    description:
      "Create polished visual art in PNG and PDF documents using layout and design constraints.",
    discovered_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  },
  {
    id: "demo-claude-api",
    source_id: "local-skills",
    name: "claude-api",
    kind: "rule",
    format: "markdown",
    relative_path: "rules/claude-api.md",
    absolute_path: "/assets/rules/claude-api.md",
    description:
      "Build apps with the Claude API or Anthropic SDK using provider-specific patterns.",
    discovered_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  },
];

export async function getOverview(): Promise<AppOverview> {
  try {
    return await invoke<AppOverview>("get_app_overview");
  } catch {
    return {
      source_count: 2,
      asset_count: fallbackAssets.length,
      profile_count: 4,
      last_scan_status: "浏览器预览模式：使用内置示例数据",
    };
  }
}

export async function listAssets(): Promise<Asset[]> {
  try {
    return await invoke<Asset[]>("list_assets");
  } catch {
    return fallbackAssets;
  }
}

export async function listSources(): Promise<Source[]> {
  try {
    return await invoke<Source[]>("list_sources");
  } catch {
    return [];
  }
}

export async function listProfiles(): Promise<TargetProfile[]> {
  try {
    return await invoke<TargetProfile[]>("list_profiles");
  } catch {
    return [];
  }
}

export async function getNavigationModel(): Promise<NavigationModel> {
  try {
    return await invoke<NavigationModel>("get_navigation_model");
  } catch {
    return navigationModel;
  }
}

export async function scanSources(): Promise<Asset[]> {
  try {
    return await invoke<Asset[]>("scan_sources");
  } catch {
    return fallbackAssets;
  }
}

export async function createPlan(profileId?: string): Promise<DeploymentPlan> {
  return await invoke<DeploymentPlan>("create_plan", { profileId });
}

export async function executePlan(plan: DeploymentPlan, actionIds?: string[]): Promise<ExecutionResult> {
  return await invoke<ExecutionResult>("execute_plan", {
    plan,
    actionIds,
  });
}

export async function revealPath(path: string): Promise<void> {
  return await invoke<void>("reveal_path", { path });
}
