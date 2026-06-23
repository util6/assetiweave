import type { Locale } from "../i18n/messages";
import type { ManualContent, ManualDocument } from "./types";

function manual(
  routeKey: string,
  zh: ManualContent,
  en: ManualContent,
): ManualDocument {
  return { routeKey, content: { zh, en } };
}

export const manualDocuments = [
  manual(
    "skills.overview",
    {
      title: "目录总览使用手册",
      subtitle: "浏览、筛选和部署 Skill 资产",
      overview: "目录总览用于查看当前扫描到的 Skill 资产、来源数量、目标 App，以及生成部署计划。",
      sections: [
        {
          heading: "常用流程",
          body: "适合第一次进入资产目录时使用，按从全局状态到单个资产的顺序建立判断。",
          outcomes: [
            "快速确认当前 Skill 资产来自哪些来源、能投放到哪些目标 App。",
            "在列表和卡片两种密度之间切换，减少大量资产时的查找成本。",
            "把编辑、分组、挂载和生成计划串成一个完整闭环。",
          ],
          items: [
            "用搜索框按名称、路径或描述过滤资产。",
            "在列表或卡片视图之间切换，选择适合当前任务的浏览密度。",
            "点击资产行上的编辑入口补充说明、调整分组或直接挂载到目标 App。",
            "点击生成部署计划，预览即将创建、更新、跳过或冲突的动作。",
          ],
          cautions: [
            "目录指标来自最近一次扫描，来源目录变更后先刷新再判断数量。",
            "生成计划只是预览，不等于已经写入目标 App。",
          ],
          keywords: ["目录", "搜索", "资产编辑", "部署计划", "Skill"],
        },
        {
          heading: "状态判断",
          body: "用于解释页面上的统计、挂载状态和冲突提示，避免把缓存状态当成真实文件系统状态。",
          outcomes: [
            "区分资产扫描状态、受管挂载状态和目标目录真实状态。",
            "在出现冲突或断链时知道先检查哪一侧。",
            "把计划里的 create、update、skip、conflict 理解为可审核动作。",
          ],
          items: [
            "来源和目标 App 指标来自最近一次目录扫描。",
            "链接状态需要点击刷新链接状态后才代表真实文件系统结果。",
            "发生冲突时先检查目标目录，避免覆盖非本应用管理的文件。",
          ],
          cautions: [
            "不要直接删除目标目录中的未知文件来解决冲突，先确认文件归属。",
            "App 专属来源通常有更严格的挂载策略，不能按普通本地源处理。",
          ],
          keywords: ["状态", "冲突", "断链", "刷新链接状态", "skip"],
        },
      ],
    },
    {
      title: "Catalog Overview Manual",
      subtitle: "Browse, filter, and deploy Skill assets",
      overview: "The catalog overview shows scanned Skill assets, source counts, target apps, and deployment plan entry points.",
      sections: [
        {
          heading: "Common workflow",
          body: "Use this flow when you first enter the catalog and need to move from global state to individual assets.",
          outcomes: [
            "Confirm where Skills come from and which target apps they can reach.",
            "Switch between list and grid density for fast scanning.",
            "Connect editing, grouping, mounting, and plan creation into one workflow.",
          ],
          items: [
            "Use search to filter by name, path, or description.",
            "Switch between list and grid views based on the density you need.",
            "Open an asset to update notes, adjust groups, or mount it to a target app.",
            "Create a deployment plan to preview create, update, skip, and conflict actions.",
          ],
          cautions: [
            "Catalog metrics come from the latest scan; refresh after source folders change.",
            "Creating a plan is a preview step and does not write target app files.",
          ],
          keywords: ["catalog", "search", "asset edit", "deployment plan", "Skill"],
        },
        {
          heading: "Reading state",
          body: "Use this section to interpret metrics, mount state, and conflict hints without mistaking cached data for filesystem truth.",
          outcomes: [
            "Separate scan state, managed mount state, and actual target directory state.",
            "Know what to inspect first when conflicts or broken links appear.",
            "Read create, update, skip, and conflict as auditable plan actions.",
          ],
          items: [
            "Source and app metrics come from the latest catalog scan.",
            "Mount state is authoritative only after refreshing real filesystem link status.",
            "Inspect target directories before resolving conflicts so unmanaged files are not overwritten.",
          ],
          cautions: [
            "Do not delete unknown target files just to clear conflicts; confirm ownership first.",
            "App-specific sources often have stricter mount policies than regular local sources.",
          ],
          keywords: ["state", "conflict", "broken link", "refresh mount state", "skip"],
        },
      ],
    },
  ),
  manual(
    "skills.groups",
    {
      title: "分组管理使用手册",
      subtitle: "按场景组织 Skill 并批量挂载",
      overview: "分组管理用于把 Skill 按项目、角色或使用场景归类，并对一组 Skill 执行批量挂载。",
      sections: [
        {
          heading: "创建和维护分组",
          body: "分组适合把同一项目、角色或工作流需要的 Skill 绑定在一起，方便筛选和批量投放。",
          outcomes: [
            "把零散 Skill 变成面向场景的集合，例如写作、前端、运维或某个项目。",
            "同时理解手动成员和规则命中的成员，避免重复维护。",
            "通过停用分组临时阻止批量操作，而不是删除分组历史。",
          ],
          items: [
            "点击新建分组，填写名称、颜色、描述并选择初始成员。",
            "分组成员可以来自手动加入，也可以来自规则命中。",
            "停用分组后，批量挂载入口会被阻止，但不会删除 Skill 文件。",
          ],
          cautions: [
            "改名和改描述不会影响 Skill 文件路径，但会影响你后续搜索和识别。",
            "停用分组不是卸载动作，已经挂载的 Skill 需要在挂载页或批量流程中处理。",
          ],
          keywords: ["分组", "场景", "手动成员", "规则成员", "停用"],
        },
        {
          heading: "批量挂载",
          body: "当一个场景组需要进入某个目标 App 时，先预览再执行，能减少跨 App 的误挂载。",
          outcomes: [
            "一次性把多个 Skill 投放到同一个目标 Profile。",
            "通过仅保留模式清理目标 App 中不属于当前场景的受管 Skill。",
            "通过新增模式保留已有挂载，只补齐缺少的 Skill。",
          ],
          items: [
            "选择一个或多个分组后，可以按目标 App 执行仅保留或新增挂载。",
            "预览对话框会列出保持、新增、卸载和跳过项。",
            "执行后建议刷新链接状态，确认目标目录与预期一致。",
          ],
          cautions: [
            "仅保留模式会卸载目标 Profile 中不在选中分组里的受管 Skill，执行前要看清预览。",
            "被来源策略阻止、冲突或断链的项会进入跳过列表，需要单独处理。",
          ],
          keywords: ["批量挂载", "仅保留", "新增", "预览", "跳过项"],
        },
      ],
    },
    {
      title: "Group Management Manual",
      subtitle: "Organize Skills by scenario and mount them in batches",
      overview: "Group management lets you classify Skills by project, role, or scenario and apply batch mount operations.",
      sections: [
        {
          heading: "Create and maintain groups",
          body: "Groups turn loose Skills into scenario-based collections that are easier to filter and mount in batches.",
          outcomes: [
            "Model a project, role, or workflow such as writing, frontend, operations, or a client project.",
            "Understand manual members and rule-matched members without duplicating upkeep.",
            "Disable a group to pause batch actions while preserving its history.",
          ],
          items: [
            "Create a group with a name, color, description, and initial members.",
            "Members can be added manually or matched by rules.",
            "Disabling a group blocks batch mount controls without deleting Skill files.",
          ],
          cautions: [
            "Renaming a group does not move Skill files, but it affects later search and recognition.",
            "Disabling is not unmounting; existing mounts must be handled from mount or batch flows.",
          ],
          keywords: ["groups", "scenario", "manual members", "rule members", "disabled"],
        },
        {
          heading: "Batch mounting",
          body: "Preview before applying a scenario group to a target app so cross-app mount mistakes are visible.",
          outcomes: [
            "Send many Skills into one target profile in a single operation.",
            "Use exclusive mode to remove managed Skills outside the selected scenario.",
            "Use additive mode to keep existing mounts and only fill gaps.",
          ],
          items: [
            "Select one or more groups, then mount them to a target app in exclusive or additive mode.",
            "The preview dialog lists kept, added, removed, and skipped items.",
            "Refresh link status after execution to verify the target directory.",
          ],
          cautions: [
            "Exclusive mode unmounts managed Skills that are not in the selected groups; review the preview.",
            "Items blocked by source policy, conflict, or broken links are skipped and need separate handling.",
          ],
          keywords: ["batch mount", "exclusive", "additive", "preview", "skipped"],
        },
      ],
    },
  ),
  manual(
    "skills.sources",
    {
      title: "技能源管理使用手册",
      subtitle: "导入、扫描和维护 Skill 来源",
      overview: "技能源管理用于维护本地目录、Git 工作区、备份库和 App 专属来源，并把这些来源扫描成资产目录。",
      sections: [
        {
          heading: "导入来源",
          body: "来源是资产目录的输入端，目录扫描只会从已登记且启用的来源中发现 Skill。",
          outcomes: [
            "把本地目录、Git 工作区、备份库或 App 专属目录纳入统一资产管理。",
            "用优先级和扫描规则控制哪些 Skill 进入目录。",
            "从远程仓库发现并获取 Skill，减少手动复制目录的成本。",
          ],
          items: [
            "点击导入源，选择根目录并设置来源名称、类型、优先级和扫描规则。",
            "点击搜索 Skill 可从远程仓库发现并获取 Skill。",
            "启用或停用来源只影响扫描与展示，不会删除源目录文件。",
          ],
          cautions: [
            "来源根目录应指向稳定位置，临时下载目录容易造成断链或重复扫描。",
            "远程获取 Skill 前先确认仓库可信，避免把不明脚本纳入日常工具链。",
          ],
          keywords: ["来源", "导入源", "远程获取", "扫描规则", "优先级"],
        },
        {
          heading: "排查扫描结果",
          body: "当目录数量、关联资产或挂载状态不符合预期时，优先从来源摘要和展开详情定位问题。",
          outcomes: [
            "确认哪些来源启用、哪些来源存在路径或规则异常。",
            "从来源展开视图追踪资产和目标 App 挂载状态。",
            "通过重新扫描把本地文件变化同步到资产目录。",
          ],
          items: [
            "来源摘要会显示总数、启用数、关联资产和异常来源。",
            "展开来源可以查看关联资产以及目标 App 的挂载状态。",
            "路径、包含规则和排除规则变更后，需要重新扫描目录。",
          ],
          cautions: [
            "停用来源不会清空磁盘文件，但会影响资产目录和后续部署计划。",
            "路径规则过宽可能把测试文件也扫描进来，过窄则会漏掉有效 Skill。",
          ],
          keywords: ["扫描", "异常来源", "包含规则", "排除规则", "挂载状态"],
        },
      ],
    },
    {
      title: "Skill Sources Manual",
      subtitle: "Import, scan, and maintain Skill sources",
      overview: "Skill sources manage local folders, Git checkouts, backup libraries, and app-specific sources that feed the asset catalog.",
      sections: [
        {
          heading: "Import sources",
          body: "Sources are the input side of the catalog; scans discover Skills only from registered and enabled sources.",
          outcomes: [
            "Bring local folders, Git worktrees, backup libraries, or app-specific folders into one catalog.",
            "Use priority and scan rules to control what enters the catalog.",
            "Discover and acquire Skills from remote repositories without manually copying folders.",
          ],
          items: [
            "Import a source by choosing a root path and setting name, type, priority, and scan rules.",
            "Use Skill discovery to acquire Skills from remote repositories.",
            "Disabling a source affects scanning and display only; it does not delete source files.",
          ],
          cautions: [
            "Point source roots at stable locations; temporary download folders often create broken links or duplicates.",
            "Confirm remote repositories are trusted before acquiring Skills into your daily toolchain.",
          ],
          keywords: ["sources", "import", "remote acquire", "scan rules", "priority"],
        },
        {
          heading: "Troubleshoot scan results",
          body: "When counts, linked assets, or mount state look wrong, start from the source summary and expanded details.",
          outcomes: [
            "Confirm which sources are enabled and which have path or rule issues.",
            "Trace assets and target app mount state from each source row.",
            "Rescan to synchronize local file changes into the asset catalog.",
          ],
          items: [
            "The summary shows total, enabled, linked assets, and problematic sources.",
            "Expand a source to inspect linked assets and target app mount states.",
            "Rescan after changing paths, include rules, or exclude rules.",
          ],
          cautions: [
            "Disabling a source does not remove files from disk, but it affects the catalog and future deployment plans.",
            "Broad rules may include test files; narrow rules may miss valid Skills.",
          ],
          keywords: ["scan", "source issues", "include rules", "exclude rules", "mount state"],
        },
      ],
    },
  ),
  manual(
    "skills.mounts",
    {
      title: "应用挂载使用手册",
      subtitle: "管理目标 App/Profile 的 Skill 链接",
      overview: "应用挂载用于配置每个目标 App 的 skills 目录，并把 Skill 以受管方式挂载或卸载到这些目录。",
      sections: [
        {
          heading: "维护目标 App",
          body: "目标 App/Profile 决定 Skill 最终写入哪里，也是受管挂载和部署计划的边界。",
          outcomes: [
            "集中维护 Codex、Claude、Cursor、OpenCode、Gemini 等目标工具的 skills 目录。",
            "为每个 Profile 配置快捷图标和目标路径，便于后续按 App 操作。",
            "通过备份库管理 App 专属来源资产，避免和普通来源混用。",
          ],
          items: [
            "点击导入 App，填写名称、App 类型、目标 skills 目录和快捷图标。",
            "默认 App 可调整路径，但删除受保护。",
            "备份库入口用于配置 Skill 备份目录，配合 App 专属来源资产挂载。",
          ],
          cautions: [
            "默认 App 受保护是为了避免误删内置投放目标，修改路径前先确认真实目录。",
            "多个 Profile 指向同一目录时，挂载状态会互相影响。",
          ],
          keywords: ["目标 App", "Profile", "skills 目录", "备份库", "默认 App"],
        },
        {
          heading: "挂载和验证",
          body: "挂载页展示按 App 和来源/分组范围组织的 Skill，执行后应刷新真实链接状态。",
          outcomes: [
            "按目标 App 快速查看哪些 Skill 已挂载、未挂载或不可挂载。",
            "通过受管符号链接创建和撤销挂载，减少手工复制文件。",
            "识别冲突、断链和策略阻止项，决定下一步处理方式。",
          ],
          items: [
            "按 App 或来源/分组范围查看可挂载 Skill。",
            "点击挂载按钮后，后端会创建并校验符号链接。",
            "刷新链接状态会重新扫描真实文件系统，区分已挂载、未挂载、冲突和断链。",
          ],
          cautions: [
            "冲突通常表示目标位置已有非受管文件，确认来源后再覆盖或迁移。",
            "断链表示目标链接指向不存在的源文件，先检查来源是否被移动或删除。",
          ],
          keywords: ["挂载", "符号链接", "冲突", "断链", "刷新链接状态"],
        },
      ],
    },
    {
      title: "App Mounts Manual",
      subtitle: "Manage Skill links for target apps and profiles",
      overview: "App mounts configure target skills directories and mount or unmount Skills through managed links.",
      sections: [
        {
          heading: "Maintain target apps",
          body: "Target apps and profiles decide where Skills are written and define the boundary for managed mounts and plans.",
          outcomes: [
            "Maintain skills directories for Codex, Claude, Cursor, OpenCode, Gemini, and other target tools.",
            "Configure shortcut icons and target paths so later operations can be app-scoped.",
            "Use the backup library for app-specific source assets without mixing them with regular sources.",
          ],
          items: [
            "Import an app with name, app type, target skills directory, and shortcut icon.",
            "Default apps can have paths adjusted, but deletion is protected.",
            "The backup library supports mounting app-specific source assets safely.",
          ],
          cautions: [
            "Default apps are protected to avoid deleting built-in targets; confirm real directories before changing paths.",
            "Profiles that share one target directory will affect each other's mount state.",
          ],
          keywords: ["target app", "profile", "skills directory", "backup library", "default app"],
        },
        {
          heading: "Mount and verify",
          body: "The mount page organizes Skills by app and source or group scope; refresh real link state after changes.",
          outcomes: [
            "See which Skills are mounted, missing, blocked, or problematic for each target app.",
            "Use managed symlinks to mount and unmount without copying files by hand.",
            "Identify conflict, broken-link, and policy-blocked items before taking action.",
          ],
          items: [
            "Browse mountable Skills by app, source, or group scope.",
            "Mount actions create and verify symlinks through the backend.",
            "Refresh link status to rescan the filesystem and distinguish mounted, missing, conflict, and broken links.",
          ],
          cautions: [
            "A conflict usually means an unmanaged file already exists at the target path; confirm ownership first.",
            "A broken link means the target points at a missing source file; check whether the source moved or was deleted.",
          ],
          keywords: ["mount", "symlink", "conflict", "broken link", "refresh mount state"],
        },
      ],
    },
  ),
  manual(
    "conversations.sessions",
    {
      title: "Session 浏览使用手册",
      subtitle: "同步、浏览和导出历史对话",
      overview: "Session 浏览是对话记录下保留的主入口，用于同步外部来源、按 App 浏览 Session、审阅问题内容，并说明来源、适配器和 aICLI/assetiweave-cli 的使用方式。",
      sections: [
        {
          heading: "同步和浏览",
          body: "先同步来源，再按 App、Session、问题三级结构浏览，适合从大量历史记录中定位一次上下文。",
          outcomes: [
            "把不同工具的对话记录整理成统一的 Session 和问题列表。",
            "从 App 列表进入 Session，再打开详情工作区查看完整内容。",
            "用全局搜索和详情搜索分别缩小 Session 和问题范围。",
          ],
          items: [
            "点击同步会读取已启用来源，并刷新 App、Session 和问题统计。",
            "左侧选择 App，中间选择 Session，右侧查看问题列表与内容。",
            "搜索框可分别过滤 Session 或当前 Session 内的问题。",
          ],
          cautions: [
            "同步依赖已启用来源和 adapter 绑定，来源禁用时不会导入新记录。",
            "详情视图中的搜索只过滤当前 Session 的问题，不会重新查询全部来源。",
          ],
          keywords: ["Session", "同步", "App 列表", "问题搜索", "详情工作区"],
        },
        {
          heading: "整理和导出",
          body: "详情工作区用于审阅回答块、命令、代码和执行结果，并把需要沉淀的内容导出。",
          outcomes: [
            "只显示当前需要看的内容块类型，减少长对话里的噪音。",
            "按问题勾选后批量导出，或者导出完整 Session。",
            "通过合并和拆分修正标准化后的问题边界。",
          ],
          items: [
            "详情视图可以按回答、命令、代码、执行结果切换内容块显示。",
            "选择多个问题后可批量导出，也可导出整个 Session。",
            "合并和拆分问题会改写标准化记录，执行前先确认当前选区。",
          ],
          cautions: [
            "合并和拆分会改变标准化记录，批量操作前先确认选区和相邻问题。",
            "导出目录默认在桌面路径下，交付前检查是否包含敏感命令或路径。",
          ],
          keywords: ["导出", "内容块", "批量选择", "合并", "拆分"],
        },
        {
          heading: "来源和适配器",
          body: "对话来源和适配器不再作为独立页面展示，它们是同步链路里的配置层：来源定义读哪里，适配器定义怎么把外部记录解析成 Session、Turn 和 Part。",
          outcomes: [
            "理解 source、adapter、Session 三者的关系，避免把同步失败误判成浏览页面问题。",
            "知道 Codex 和 OpenCode 保留内置兜底；Claude Code、ZCode 等来源通过外部插件参与同步。",
            "知道新增外部 adapter 后，需要让来源绑定正确的 adapter_id 才能导入数据。",
          ],
          items: [
            "来源包含 id、adapter_id、name、kind、location、config_json 和 enabled 等字段。",
            "adapter_id 必须指向一个已存在且可用的适配器；除 Codex/OpenCode 兜底外，应优先走外部插件。",
            "Session 浏览页点击同步时，会读取已启用来源；禁用来源不会导入新记录。",
            "外部适配器需要通过 manifest 声明协议版本、命令入口、能力和输入来源类型。",
          ],
          cautions: [
            "来源路径存在但 adapter_id 绑定错误时，导入结果可能为空。",
            "未注册、未受信任、内容 hash 已变化或未启用的外部 adapter 不应进入日常同步。",
            "修改 manifest 或脚本后，先重新 validate 和 try-run，再进入正式同步。",
          ],
          keywords: ["对话来源", "适配器", "adapter_id", "manifest", "同步链路"],
        },
        {
          heading: "aICLI / assetiweave-cli",
          body: "这里的 aICLI 指外部 AI 工具、Skill 或人工脚本通过 assetiweave-cli 调用 AssetIWeave 的对话能力；UI 只负责浏览和同步，来源与适配器配置主要走 CLI。",
          outcomes: [
            "用命令查看当前 adapter 和 source 状态。",
            "按 scaffold、validate、register、source add、try-run 的顺序接入外部解析器。",
            "用 source add/update/disable 和 sync 控制哪些记录进入对话库。",
          ],
          items: [
            "先列出现有适配器：assetiweave-cli conversation adapter list。",
            "创建外部适配器骨架：assetiweave-cli conversation adapter scaffold --directory ~/.assetiweave/conversation-adapters/my-adapter --id my-app --name \"My App\"。",
            "校验 manifest：assetiweave-cli conversation adapter validate ~/.assetiweave/conversation-adapters/my-adapter/conversation-adapter.json。",
            "试运行读取能力：assetiweave-cli conversation adapter try-run ~/.assetiweave/conversation-adapters/my-adapter/conversation-adapter.json --method read_session --location ~/my-app-records --yes。",
            "注册适配器并新增来源：assetiweave-cli conversation adapter register ~/.assetiweave/conversation-adapters/my-adapter/conversation-adapter.json --yes；assetiweave-cli conversation source add --id my-app-live --adapter my-app --name \"My App Live\" --kind directory --location ~/my-app-records。",
            "预览同步：assetiweave-cli conversation sync --adapter my-app --dry-run；确认后去掉 --dry-run 正式导入。",
          ],
          cautions: [
            "register 和 try-run 会信任或执行外部脚本，确认来源可信后再加 --yes。",
            "kind 只能使用 live、file、directory、sqlite 或 custom，且要匹配 adapter manifest 支持的 input_kinds。",
            "开发中的 adapter 建议先用测试目录和 --dry-run 验证，避免污染正式对话库。",
          ],
          keywords: ["aICLI", "assetiweave-cli", "scaffold", "validate", "register", "source add", "sync"],
        },
      ],
    },
    {
      title: "Session Browser Manual",
      subtitle: "Sync, browse, and export conversation history",
      overview: "The session browser is the remaining Conversations entry point for syncing external sources, browsing sessions by app, reviewing question content, and documenting source, adapter, and aICLI/assetiweave-cli usage.",
      sections: [
        {
          heading: "Sync and browse",
          body: "Sync sources first, then browse by app, session, and question to locate a conversation context.",
          outcomes: [
            "Normalize conversation records from different tools into sessions and questions.",
            "Move from app list to session list, then open the detail workspace.",
            "Use global and detail search to narrow sessions and questions separately.",
          ],
          items: [
            "Sync reads enabled sources and refreshes app, session, and question statistics.",
            "Select an app, then a session, then inspect questions and content.",
            "Search filters either sessions or questions inside the active session.",
          ],
          cautions: [
            "Sync depends on enabled sources and adapter bindings; disabled sources import nothing new.",
            "Detail search filters only the active session and does not query every source again.",
          ],
          keywords: ["Session", "sync", "app list", "question search", "detail workspace"],
        },
        {
          heading: "Organize and export",
          body: "Use the detail workspace to review answer blocks, commands, code, results, and export what should be kept.",
          outcomes: [
            "Show only the content block types you need in long conversations.",
            "Export selected questions in bulk or export the full session.",
            "Correct normalized question boundaries with merge and split actions.",
          ],
          items: [
            "Toggle answer, command, code, and result content blocks in detail view.",
            "Export selected questions or the full session.",
            "Merge and split mutate normalized records, so confirm the active selection first.",
          ],
          cautions: [
            "Merge and split change normalized records; verify selected and adjacent questions first.",
            "The export directory defaults under Desktop, so check sensitive commands or paths before sharing.",
          ],
          keywords: ["export", "content blocks", "bulk selection", "merge", "split"],
        },
        {
          heading: "Sources and adapters",
          body: "Conversation sources and adapters are no longer separate screens. They are the configuration layer of sync: sources define where to read, and adapters define how external records become sessions, turns, and parts.",
          outcomes: [
            "Understand the source, adapter, and session relationship before debugging sync.",
            "Know that Codex and OpenCode keep built-in fallback, while sources such as Claude Code and ZCode sync through external plugins.",
            "Know that each new external adapter needs sources bound to the correct adapter_id.",
          ],
          items: [
            "A source includes id, adapter_id, name, kind, location, config_json, and enabled fields.",
            "adapter_id must point to an existing usable adapter; except for Codex/OpenCode fallback, prefer external plugins.",
            "The Sync action reads enabled sources; disabled sources do not import new records.",
            "An external adapter manifest declares protocol version, command entry, capabilities, and supported input kinds.",
          ],
          cautions: [
            "A valid source path can still import nothing when adapter_id is wrong.",
            "External adapters that are unregistered, untrusted, hash-changed, or disabled should not enter routine sync.",
            "After changing a manifest or script, validate and try-run before real sync.",
          ],
          keywords: ["conversation source", "adapter", "adapter_id", "manifest", "sync pipeline"],
        },
        {
          heading: "aICLI / assetiweave-cli",
          body: "Here aICLI means an external AI tool, Skill, or human script calling AssetIWeave through assetiweave-cli. The UI handles browsing and sync, while source and adapter setup primarily happens through the CLI.",
          outcomes: [
            "Inspect current adapter and source state with commands.",
            "Connect an external parser through scaffold, validate, register, source add, and try-run.",
            "Use source add/update/disable and sync to control what enters the conversation library.",
          ],
          items: [
            "List adapters: assetiweave-cli conversation adapter list.",
            "Create a skeleton: assetiweave-cli conversation adapter scaffold --directory ~/.assetiweave/conversation-adapters/my-adapter --id my-app --name \"My App\".",
            "Validate the manifest: assetiweave-cli conversation adapter validate ~/.assetiweave/conversation-adapters/my-adapter/conversation-adapter.json.",
            "Try a read method: assetiweave-cli conversation adapter try-run ~/.assetiweave/conversation-adapters/my-adapter/conversation-adapter.json --method read_session --location ~/my-app-records --yes.",
            "Register the adapter and add a source: assetiweave-cli conversation adapter register ~/.assetiweave/conversation-adapters/my-adapter/conversation-adapter.json --yes; assetiweave-cli conversation source add --id my-app-live --adapter my-app --name \"My App Live\" --kind directory --location ~/my-app-records.",
            "Preview sync: assetiweave-cli conversation sync --adapter my-app --dry-run; remove --dry-run to import.",
          ],
          cautions: [
            "register and try-run trust or execute external scripts; use --yes only after reviewing the source.",
            "kind must be live, file, directory, sqlite, or custom, and must match adapter manifest input_kinds.",
            "Validate development adapters against test folders and --dry-run before touching the production conversation library.",
          ],
          keywords: ["aICLI", "assetiweave-cli", "scaffold", "validate", "register", "source add", "sync"],
        },
      ],
    },
  ),
  manual(
    "conversations.web-records",
    {
      title: "网页记录浏览使用手册",
      subtitle: "采集、同步和管理网页版 AI 对话",
      overview: "网页记录浏览只展示由用户目录采集脚本导入的网页版 AI 对话。解析脚本保存在 .assetiweave 下，应用负责执行、标准化，并把结果写入独立的 web_record 数据表。",
      sections: [
        {
          heading: "采集链路",
          items: [
            "网页 adapter 插件位于 ~/.assetiweave/conversation-adapters，应用代码不包含站点私有解析细节。",
            "adapter manifest 通过 web_records capability 声明数据应进入网页记录仓储。",
            "同步前必须先由采集脚本确认浏览器登录状态，并生成标准化对话数据。",
          ],
          cautions: [
            "网页接口和认证方式可能变化，脚本更新后应先执行 auth-check 和测试同步。",
            "只运行已审查的用户目录脚本，避免把浏览器凭据交给不可信代码。",
          ],
        },
        {
          heading: "浏览和导出",
          items: [
            "页面按站点、网页对话和问题三级结构展示，布局与 Session 浏览保持一致。",
            "网页记录使用独立表，不会出现在 Session 浏览列表中。",
            "可搜索网页对话内容，并导出完整记录或选中的问题为 Markdown。",
          ],
        },
        {
          heading: "CLI",
          items: [
            "安装网页插件来源：assetiweave-cli conversation adapter register ~/.assetiweave/conversation-adapters/chatgpt-web/conversation-adapter.json --yes；assetiweave-cli conversation source add --id chatgpt-web-export --adapter chatgpt-web --name \"ChatGPT Web\" --kind directory --location ~/.assetiweave/conversation-adapters/chatgpt-web/output/normalized。",
            "同步网页来源：assetiweave-cli conversation sync --adapter chatgpt-web --record-kind web。",
            "列出网页记录：assetiweave-cli conversation web-record list。",
            "查看或导出：assetiweave-cli conversation web-record get <record-id>；assetiweave-cli conversation web-record export <record-id> --output-root <dir>。",
          ],
        },
      ],
    },
    {
      title: "Web Record Browser Manual",
      subtitle: "Harvest, sync, and manage AI web conversations",
      overview: "The web record browser only shows conversations imported by user-directory web harvesters. Scripts live under .assetiweave, while the app executes adapters, normalizes output, and stores it in independent web_record tables.",
      sections: [
        {
          heading: "Harvest pipeline",
          items: [
            "Web adapter plugins live under ~/.assetiweave/conversation-adapters, outside application code.",
            "The adapter manifest declares web_records so sync selects the independent web record repository.",
            "The harvester must verify browser login state before producing normalized conversation data.",
          ],
          cautions: [
            "Web APIs and authentication can change; run auth-check and a test sync after script updates.",
            "Only execute reviewed user-directory scripts because they may access browser credentials.",
          ],
        },
        {
          heading: "Browse and export",
          items: [
            "Browse by site, web conversation, and question using the same structure as the session browser.",
            "Web records use independent tables and never appear in the Session browser.",
            "Search content and export a full record or selected questions as Markdown.",
          ],
        },
        {
          heading: "CLI",
          items: [
            "Install a web plugin source: assetiweave-cli conversation adapter register ~/.assetiweave/conversation-adapters/chatgpt-web/conversation-adapter.json --yes; assetiweave-cli conversation source add --id chatgpt-web-export --adapter chatgpt-web --name \"ChatGPT Web\" --kind directory --location ~/.assetiweave/conversation-adapters/chatgpt-web/output/normalized.",
            "Sync a web adapter: assetiweave-cli conversation sync --adapter chatgpt-web --record-kind web.",
            "List records: assetiweave-cli conversation web-record list.",
            "Inspect or export: assetiweave-cli conversation web-record get <record-id>; assetiweave-cli conversation web-record export <record-id> --output-root <dir>.",
          ],
        },
      ],
    },
  ),
  manual(
    "mcp.overview",
    {
      title: "服务总览使用手册",
      subtitle: "查看 MCP 服务资产和配置状态",
      overview: "服务总览用于汇总 MCP Server、配置片段和目标应用投影的整体状态；当前页面仍在建设中，本手册先固定入口和操作边界。",
      sections: [
        {
          heading: "当前可用",
          items: [
            "页头问号会打开本页面说明，返回按钮可回到建设中界面。",
            "页面主体显示建设中状态和 routeKey，说明导航与手册注册已经接入。",
            "后续 MCP 功能落地后，本说明会继续作为服务总览的入口文档。",
          ],
        },
        {
          heading: "落地后的检查顺序",
          items: [
            "先查看已登记 Server、配置片段和目标 App 的数量。",
            "再检查哪些配置可投影、哪些缺少路径或凭据。",
            "最后进入服务管理或配置投影页面处理异常项。",
          ],
        },
      ],
    },
    {
      title: "Service Overview Manual",
      subtitle: "Review MCP service assets and configuration state",
      overview: "The service overview will summarize MCP servers, configuration fragments, and target app projections; the screen is under construction, and this guide fixes the entry point and workflow boundary.",
      sections: [
        {
          heading: "Available now",
          items: [
            "The header help button opens this manual, and the back button returns to the under-construction screen.",
            "The body shows the construction state and routeKey, proving navigation and manual registration are wired.",
            "When MCP features ship, this guide remains the service overview entry document.",
          ],
        },
        {
          heading: "Future review order",
          items: [
            "Check registered server, fragment, and target app counts first.",
            "Identify which configs can project and which are missing paths or credentials.",
            "Use server management or config projection pages to resolve exceptions.",
          ],
        },
      ],
    },
  ),
  manual(
    "mcp.servers",
    {
      title: "服务管理使用手册",
      subtitle: "登记和维护 MCP Server 定义",
      overview: "服务管理用于维护 MCP Server 的名称、命令、环境变量、启用状态和来源归属；当前页面保留入口，等待服务模型落地。",
      sections: [
        {
          heading: "当前可用",
          items: [
            "从 MCP 顶部分类进入服务管理，可以看到建设中状态。",
            "问号入口会打开本说明，说明内容与 mcp.servers routeKey 绑定。",
            "在功能未完成前，不会写入任何 Server 配置或目标 App 文件。",
          ],
        },
        {
          heading: "落地后的使用流程",
          items: [
            "新增 Server 时先填写显示名称、启动命令和工作目录。",
            "为需要隔离的变量配置环境变量，不把密钥写进普通说明字段。",
            "启用前先做本地校验，再进入配置投影页面生成目标 App 配置。",
          ],
        },
      ],
    },
    {
      title: "Server Management Manual",
      subtitle: "Register and maintain MCP server definitions",
      overview: "Server management will maintain MCP server names, commands, environment variables, enabled state, and source ownership; the route is present while the service model is still being implemented.",
      sections: [
        {
          heading: "Available now",
          items: [
            "Open Server Management from the MCP category to see the construction state.",
            "The help entry opens this route-specific manual for mcp.servers.",
            "Before the feature ships, no server config or target app file is written.",
          ],
        },
        {
          heading: "Future workflow",
          items: [
            "Create a server with display name, launch command, and working directory.",
            "Put isolated variables in environment settings instead of plain description fields.",
            "Validate locally before generating target app configuration projections.",
          ],
        },
      ],
    },
  ),
  manual(
    "mcp.configs",
    {
      title: "配置投影使用手册",
      subtitle: "把 MCP 配置安全写入目标 App",
      overview: "配置投影用于把已登记的 MCP Server 组合成目标 App 可识别的配置文件，并在写入前展示差异、冲突和跳过原因。",
      sections: [
        {
          heading: "当前可用",
          items: [
            "页面入口已经接入导航和手册系统，但投影执行器仍未开放。",
            "建设中页面只显示 routeKey，不会创建、覆盖或删除任何配置文件。",
            "后续实现会沿用部署计划的预览优先原则。",
          ],
        },
        {
          heading: "落地后的使用流程",
          items: [
            "选择目标 App/Profile，预览将写入的 MCP 配置片段。",
            "逐项检查 create、update、skip、conflict 的原因。",
            "只在确认非受管文件不会被覆盖后执行投影。",
          ],
        },
      ],
    },
    {
      title: "Config Projection Manual",
      subtitle: "Project MCP config safely into target apps",
      overview: "Config projection will compose registered MCP servers into target app configuration files and show diffs, conflicts, and skip reasons before writing.",
      sections: [
        {
          heading: "Available now",
          items: [
            "Navigation and manuals are wired, but the projection executor is not exposed yet.",
            "The construction page only shows the routeKey and does not create, overwrite, or delete files.",
            "The shipped workflow will keep the deployment-plan-first safety model.",
          ],
        },
        {
          heading: "Future workflow",
          items: [
            "Select a target app/profile and preview generated MCP fragments.",
            "Review create, update, skip, and conflict reasons item by item.",
            "Execute only after unmanaged target files are confirmed safe.",
          ],
        },
      ],
    },
  ),
  manual(
    "prompts.overview",
    {
      title: "提示词总览使用手册",
      subtitle: "查看和筛选 Prompt 资产",
      overview: "提示词总览用于集中浏览 prompt 类资产、来源和挂载状态，并把提示词后续纳入与 Skill 相同的资产管理模型。",
      sections: [
        {
          heading: "当前可用",
          items: [
            "该 routeKey 已启用，当前显示建设中状态。",
            "问号入口说明提示词页面的目标边界，避免误跳到 Skill 目录总览。",
            "现阶段不会自动扫描或部署 prompt 专属视图外的内容。",
          ],
        },
        {
          heading: "落地后的使用流程",
          items: [
            "先按来源、标签或路径筛选 prompt 资产。",
            "检查每条 prompt 的类型、描述、兼容目标和启用状态。",
            "需要给某个 App 使用时，再进入目标应用或挂载流程。",
          ],
        },
      ],
    },
    {
      title: "Prompt Overview Manual",
      subtitle: "Review and filter Prompt assets",
      overview: "Prompt overview will centralize prompt assets, sources, and mount state so prompts can join the same asset management model as Skills.",
      sections: [
        {
          heading: "Available now",
          items: [
            "The routeKey is enabled and currently shows the construction state.",
            "The help entry documents the prompt page boundary instead of falling back to the Skill catalog.",
            "This stage does not scan or deploy prompt-specific views automatically.",
          ],
        },
        {
          heading: "Future workflow",
          items: [
            "Filter prompt assets by source, tag, or path.",
            "Check each prompt's type, description, target compatibility, and enabled state.",
            "Move to target app or mount workflows when a prompt should be used by an app.",
          ],
        },
      ],
    },
  ),
  manual(
    "prompts.templates",
    {
      title: "模板管理使用手册",
      subtitle: "维护可复用 Prompt 模板",
      overview: "模板管理用于维护可复用的 prompt 模板、变量占位、默认说明和适用场景，避免把模板正文散落在多个工具目录里。",
      sections: [
        {
          heading: "当前可用",
          items: [
            "模板管理目前是建设中页面，只展示 routeKey 和说明入口。",
            "手册先记录模板页面与普通 prompt 总览的区别。",
            "功能完成前不会修改任何模板文件或生成目标文件。",
          ],
        },
        {
          heading: "落地后的使用流程",
          items: [
            "创建模板时先定义名称、用途、变量和默认正文。",
            "按项目、角色或输出格式给模板打标签。",
            "发布到目标 App 前先预览变量替换后的结果。",
          ],
        },
      ],
    },
    {
      title: "Template Management Manual",
      subtitle: "Maintain reusable Prompt templates",
      overview: "Template management will maintain reusable prompt templates, variable placeholders, default notes, and scenarios without scattering template bodies across tool folders.",
      sections: [
        {
          heading: "Available now",
          items: [
            "Template Management currently shows the construction state, routeKey, and help entry.",
            "This manual records the difference between templates and the regular prompt overview.",
            "No template file or generated target file is modified before the feature ships.",
          ],
        },
        {
          heading: "Future workflow",
          items: [
            "Create templates with name, purpose, variables, and default body.",
            "Tag templates by project, role, or output format.",
            "Preview variable substitution before publishing to a target app.",
          ],
        },
      ],
    },
  ),
  manual(
    "prompts.targets",
    {
      title: "目标应用使用手册",
      subtitle: "管理 Prompt 的目标 App 投放",
      overview: "目标应用页面用于决定哪些 prompt 或模板可以投放到哪些 App/Profile，并显示目标目录、兼容性和投影风险。",
      sections: [
        {
          heading: "当前可用",
          items: [
            "当前页面只确认 prompts.targets 的导航和说明入口。",
            "任何目标 App 写入仍由后续投影或部署流程负责。",
            "在未完成前，请继续通过已有 Skill 挂载页面管理已实现的挂载关系。",
          ],
        },
        {
          heading: "落地后的使用流程",
          items: [
            "选择目标 App/Profile，查看支持的 prompt 格式和目标路径。",
            "为 prompt 或模板选择启用、禁用或仅预览。",
            "通过部署计划确认目标文件变化后再执行。",
          ],
        },
      ],
    },
    {
      title: "Target Apps Manual",
      subtitle: "Manage Prompt delivery into target apps",
      overview: "Target Apps will decide which prompts or templates can project into which app/profile, while showing target directories, compatibility, and projection risk.",
      sections: [
        {
          heading: "Available now",
          items: [
            "The current page confirms navigation and help entry for prompts.targets.",
            "Target app writes remain owned by future projection or deployment flows.",
            "Use the existing Skill mount pages for implemented mount relationships for now.",
          ],
        },
        {
          heading: "Future workflow",
          items: [
            "Select a target app/profile and review supported prompt formats and paths.",
            "Enable, disable, or preview each prompt or template.",
            "Confirm target file changes through the deployment plan before execution.",
          ],
        },
      ],
    },
  ),
  manual(
    "rules.overview",
    {
      title: "规则总览使用手册",
      subtitle: "查看 Rule 资产和策略覆盖范围",
      overview: "规则总览用于集中查看 rule 类资产、启用范围、目标工具兼容性和潜在冲突，帮助用户理解哪些规则会影响部署结果。",
      sections: [
        {
          heading: "当前可用",
          items: [
            "规则总览入口已启用，当前显示建设中状态。",
            "手册入口独立于 Layout 和 Router，内容保存在 manuals 目录。",
            "未实现前不会改变任何策略或目标工具规则文件。",
          ],
        },
        {
          heading: "落地后的使用流程",
          items: [
            "先按来源、规则类型和目标 App 查看规则资产。",
            "检查规则是否启用，以及是否与 Profile 能力匹配。",
            "发现重叠规则时进入冲突检测页查看处理建议。",
          ],
        },
      ],
    },
    {
      title: "Rule Overview Manual",
      subtitle: "Review Rule assets and policy coverage",
      overview: "Rule overview will centralize rule assets, enablement scope, target tool compatibility, and potential conflicts so users understand which rules affect deployment output.",
      sections: [
        {
          heading: "Available now",
          items: [
            "The Rule Overview route is enabled and currently shows the construction state.",
            "The manual entry is independent from Layout and Router, with content stored in manuals.",
            "No policy or target rule file is changed before implementation.",
          ],
        },
        {
          heading: "Future workflow",
          items: [
            "Review rule assets by source, rule type, and target app.",
            "Check whether each rule is enabled and compatible with profile capabilities.",
            "Use Conflict Detection when overlapping rules need resolution.",
          ],
        },
      ],
    },
  ),
  manual(
    "rules.policies",
    {
      title: "启用策略使用手册",
      subtitle: "配置规则如何参与部署决策",
      overview: "启用策略页面用于表达 rule 资产的 include/exclude 条件、优先级和 Profile 覆盖关系，确保部署决策可解释。",
      sections: [
        {
          heading: "当前可用",
          items: [
            "页面已经有导航入口和说明入口，但策略编辑器尚未开放。",
            "建设中状态不会写入策略、覆盖现有规则或触发部署计划。",
            "后续实现会复用当前的决策解释和部署预览约束。",
          ],
        },
        {
          heading: "落地后的使用流程",
          items: [
            "先选择策略作用域，例如来源、标签、分组或目标 Profile。",
            "再设置 include/exclude 条件和优先级。",
            "保存后通过解释视图确认每条规则为什么被部署或跳过。",
          ],
        },
      ],
    },
    {
      title: "Policy Management Manual",
      subtitle: "Configure how rules participate in deployment decisions",
      overview: "Policy Management will express include/exclude conditions, priority, and profile overrides for rule assets so deployment decisions remain explainable.",
      sections: [
        {
          heading: "Available now",
          items: [
            "Navigation and help entries exist, but the policy editor is not exposed yet.",
            "The construction state does not write policies, overwrite rules, or trigger plans.",
            "The shipped implementation will reuse decision explanations and plan preview constraints.",
          ],
        },
        {
          heading: "Future workflow",
          items: [
            "Choose a policy scope such as source, tag, group, or target profile.",
            "Set include/exclude conditions and priority.",
            "Use the explanation view to confirm why each rule deploys or skips.",
          ],
        },
      ],
    },
  ),
  manual(
    "rules.conflicts",
    {
      title: "冲突检测使用手册",
      subtitle: "发现重叠规则和目标文件风险",
      overview: "冲突检测用于在执行前发现多个 rule 资产对同一目标、同一文件或同一策略产生的重叠影响，并给出可解释的处理入口。",
      sections: [
        {
          heading: "当前可用",
          items: [
            "当前页面保留 routeKey、建设中状态和手册入口。",
            "未实现前不会扫描真实目标目录或修改规则关系。",
            "已有部署计划中的文件冲突仍在当前 Skill 挂载链路中展示。",
          ],
        },
        {
          heading: "落地后的使用流程",
          items: [
            "先运行冲突扫描，查看按目标 App 分组的风险项。",
            "检查冲突原因：同名文件、策略重叠、优先级不明确或非受管文件。",
            "调整策略或挂载关系后重新扫描，直到风险项清晰可处理。",
          ],
        },
      ],
    },
    {
      title: "Conflict Detection Manual",
      subtitle: "Find overlapping rules and target file risk",
      overview: "Conflict Detection will identify overlapping effects from multiple rule assets on the same target, file, or policy before execution and provide explainable resolution entry points.",
      sections: [
        {
          heading: "Available now",
          items: [
            "The current page preserves routeKey, construction state, and manual entry.",
            "Before implementation, it does not scan target directories or mutate rule relationships.",
            "Existing deployment-plan file conflicts remain visible in the Skill mount workflow.",
          ],
        },
        {
          heading: "Future workflow",
          items: [
            "Run a conflict scan and review risks grouped by target app.",
            "Inspect causes such as same-name files, policy overlap, unclear priority, or unmanaged files.",
            "Adjust policies or mounts and rescan until risks are explainable.",
          ],
        },
      ],
    },
  ),
  manual(
    "profiles.overview",
    {
      title: "应用总览使用手册",
      subtitle: "查看目标 App/Profile 的整体状态",
      overview: "应用总览用于集中查看所有目标 App/Profile 的路径、支持资产类型、挂载数量和最近部署状态，是目标配置的入口页。",
      sections: [
        {
          heading: "当前可用",
          items: [
            "当前 profiles.overview 只展示建设中页面和说明入口。",
            "已有 App 快捷图标和挂载状态仍在 Skill 相关页面可用。",
            "本页完成前，请通过挂载管理和设置中的部署面板维护目标路径。",
          ],
        },
        {
          heading: "落地后的使用流程",
          items: [
            "查看每个 Profile 的名称、App 类型、目标路径和启用状态。",
            "确认支持的资产类型是否符合当前部署目标。",
            "从异常项进入模板、计划或设置页面继续处理。",
          ],
        },
      ],
    },
    {
      title: "App Overview Manual",
      subtitle: "Review target app/profile state",
      overview: "App Overview will centralize target app/profile paths, supported asset kinds, mount counts, and recent deployment state as the target configuration entry page.",
      sections: [
        {
          heading: "Available now",
          items: [
            "profiles.overview currently shows construction state and the manual entry.",
            "Existing app shortcut icons and mount state remain available in Skill pages.",
            "Until this page ships, maintain target paths through Mount Management and deployment settings.",
          ],
        },
        {
          heading: "Future workflow",
          items: [
            "Review each profile's name, app type, target path, and enabled state.",
            "Confirm supported asset kinds match the deployment target.",
            "Open templates, plans, or settings from any exception item.",
          ],
        },
      ],
    },
  ),
  manual(
    "profiles.templates",
    {
      title: "配置模板使用手册",
      subtitle: "用模板创建目标 Profile",
      overview: "配置模板用于维护 Codex、Claude、Cursor、OpenCode、Gemini、Antigravity、OpenClaw 和自定义工具的 Profile 起始配置。",
      sections: [
        {
          heading: "当前可用",
          items: [
            "模板页面当前未开放编辑器，只保留导航和说明。",
            "默认 Profile 模板由后端 seed 和设置面板共同支撑。",
            "未实现前不会改变已有 Profile 或 App 快捷入口配置。",
          ],
        },
        {
          heading: "落地后的使用流程",
          items: [
            "选择内置模板或复制已有 Profile 作为起点。",
            "确认目标路径、支持资产类型和默认部署策略。",
            "保存前预览将创建的 Profile 字段，避免路径指向错误目录。",
          ],
        },
      ],
    },
    {
      title: "Profile Templates Manual",
      subtitle: "Create target profiles from templates",
      overview: "Profile Templates will maintain starter profile configurations for Codex, Claude, Cursor, OpenCode, Gemini, Antigravity, OpenClaw, and custom tools.",
      sections: [
        {
          heading: "Available now",
          items: [
            "The template editor is not exposed yet; navigation and manual entry are present.",
            "Default profile templates are currently backed by backend seeds and settings panels.",
            "Existing profiles and app shortcut configuration are not changed before implementation.",
          ],
        },
        {
          heading: "Future workflow",
          items: [
            "Choose a built-in template or copy an existing profile as a starting point.",
            "Confirm target path, supported asset kinds, and default deployment strategy.",
            "Preview fields before saving so paths do not point to the wrong directory.",
          ],
        },
      ],
    },
  ),
  manual(
    "profiles.plans",
    {
      title: "部署计划使用手册",
      subtitle: "按 Profile 预览和执行部署变化",
      overview: "部署计划页面用于按目标 Profile 汇总 create、update、remove、skip 和 conflict 动作，并在执行前解释每个动作的原因。",
      sections: [
        {
          heading: "当前可用",
          items: [
            "独立 Plans 页面仍在建设中；当前 Skill 目录总览已提供基础生成部署计划入口。",
            "本 routeKey 的问号入口已接入，后续会承接完整计划视图。",
            "未完成前，请继续从目录总览生成和查看当前计划。",
          ],
        },
        {
          heading: "落地后的使用流程",
          items: [
            "选择一个或多个 Profile，生成只包含相关挂载关系的计划。",
            "按动作类型检查目标路径、原因和风险提示。",
            "执行前确认冲突项已处理，执行后查看结果和残留风险。",
          ],
        },
      ],
    },
    {
      title: "Deployment Plans Manual",
      subtitle: "Preview and execute deployment changes by profile",
      overview: "Deployment Plans will summarize create, update, remove, skip, and conflict actions by target profile and explain every action before execution.",
      sections: [
        {
          heading: "Available now",
          items: [
            "The standalone Plans page is under construction; Catalog Overview already has the basic create-plan entry.",
            "This routeKey's help entry is wired and will host the full plan view guide later.",
            "For now, continue generating and reviewing plans from Catalog Overview.",
          ],
        },
        {
          heading: "Future workflow",
          items: [
            "Select one or more profiles and generate a plan from enabled mount relationships.",
            "Review target path, reason, and risk by action type.",
            "Resolve conflicts before execution, then inspect results and residual risks.",
          ],
        },
      ],
    },
  ),
];

const manualByRouteKey = new Map(manualDocuments.map((document) => [document.routeKey, document]));

export function getManualDocument(routeKey: string): ManualDocument | undefined {
  return manualByRouteKey.get(routeKey);
}

export function hasManualDocument(routeKey: string): boolean {
  return manualByRouteKey.has(routeKey);
}

export function getManualContent(document: ManualDocument, locale: Locale): ManualContent {
  return document.content[locale] ?? document.content.zh;
}
