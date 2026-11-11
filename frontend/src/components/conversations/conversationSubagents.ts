import type {
  ConversationContentNode,
  ConversationPart,
  ConversationQuestionDetail,
  ConversationSession,
  ConversationTurn,
} from "../../types";
import { conversationIdFragment } from "../../utils/conversationIds";

/**
 * ADR-0022 的父轮次委派锚点在前端的统一视图模型。
 *
 * 数据全部取自父会话详情里已经内联的锚点 Part（`metadata_json`）、
 * 轮次过程链（`turn.process`）与投影节点正文，**不允许**为了渲染父会话
 * 再去请求任何子会话详情（严格零 N+1）。
 */
export interface SubagentDispatchPortalViewModel {
  partId: string;
  locator: string;
  /**
   * 子会话主键。历史锚点可能只有 `child_external_id`（旧适配器口径），
   * 此时为空字符串，卡片降级为只读展示，不允许跳转。
   */
  childSessionId: string;
  agentRole?: string;
  title: string;
  status: "completed" | "running" | "failed" | string;
  model?: string;
  stepCount?: number;
  reportSummary?: string;
  /** 锚点定位不到子会话：正文仍需可见（不能静默消失），但不提供跳转。 */
  unresolved?: boolean;
}

/**
 * 带轮次归属的锚点：左栏按轮次分组、正文按轮次渲染都需要知道它属于哪一问。
 */
export interface SubagentDispatchPortal extends SubagentDispatchPortalViewModel {
  turnId: string;
}

/** 子代理传送门卡片在正文里的专用渲染器标识。 */
export const SUBAGENT_PORTAL_RENDERER = "subagent_portal";

/** 子代理会话在 `execution_origin` 上的取值（与后端常量一致）。 */
export const SUBAGENT_EXECUTION_ORIGIN = "subagent";

/** 打开某个子代理会话时携带的父会话溯源上下文。 */
export interface SubagentSessionTarget {
  agentRole?: string | null;
  childSessionId: string;
  parentQuestionId: string;
  parentSessionId: string;
  parentTurnId: string;
  parentTurnIndex: number;
}

/** 子会话详情顶栏的返回面包屑。 */
export interface ParentSessionBreadcrumb {
  agentRole?: string;
  parentSessionId: string;
  parentShortId: string;
  /** 父轮次在父会话内的 0 基位置；用于回跳后精确定位。 */
  parentTurnIndex?: number;
  /** 人类可读的父轮次序号（1 基）。 */
  parentTurnNumber?: number;
}

type JsonObject = Record<string, unknown>;

/**
 * ADR-0022 需要剔除的伪轮次：`subagent` 曾被拆成独立物理轮次，
 * `continuation` 是它紧随其后的续接活动。二者都不属于人类提问。
 */
export function isLegacyPseudoTurn(
  turn: Pick<ConversationTurn, "turn_kind" | "parent_turn_id">,
): boolean {
  const kind = (turn.turn_kind ?? "").trim().toLowerCase();
  if (kind === "subagent" || kind === "continuation" || kind === "agent")
    return true;
  // 早期数据可能只写了 parent_turn_id、没有 turn_kind，同样按伪轮次处理。
  return Boolean((turn.parent_turn_id ?? "").trim());
}

/** 只保留真实人类提问轮次，按 turn_index 稳定排序。 */
export function mainConversationTurns(
  question: ConversationQuestionDetail,
): ConversationTurn[] {
  return question.turns
    .filter((turn) => !isLegacyPseudoTurn(turn))
    .slice()
    .sort(
      (left, right) =>
        (left.turn_index ?? 0) - (right.turn_index ?? 0) ||
        left.id.localeCompare(right.id),
    );
}

/** 某个父轮次派发的全部子代理锚点。 */
export function subagentPortalsForTurn(
  question: ConversationQuestionDetail,
  turnId: string,
): SubagentDispatchPortal[] {
  return collectSubagentPortals(question).filter(
    (portal) => portal.turnId === turnId,
  );
}

/** 便捷视图：`turnId -> 该轮派发的锚点列表`。 */
export function groupSubagentPortalsByTurn(
  question: ConversationQuestionDetail,
): Map<string, SubagentDispatchPortal[]> {
  const grouped = new Map<string, SubagentDispatchPortal[]>();
  for (const portal of collectSubagentPortals(question)) {
    const entries = grouped.get(portal.turnId) ?? [];
    entries.push(portal);
    grouped.set(portal.turnId, entries);
  }
  return grouped;
}

interface AnchoredPortal extends SubagentDispatchPortal {
  partIndex: number;
  turnIndex: number;
}

/**
 * 从父会话详情提取全部可跳转的子代理锚点，按轮次与零件顺序稳定排列。
 */
export function collectSubagentPortals(
  question: ConversationQuestionDetail,
): SubagentDispatchPortal[] {
  const portals: AnchoredPortal[] = [];
  const seenPartIds = new Set<string>();
  const nodeLocatorByPartId = new Map<string, string>();

  for (const node of question.projected_content_nodes ?? []) {
    if (!isSubagentAnchorNode(node)) continue;
    nodeLocatorByPartId.set(node.part_id, contentNodeLocator(node));
  }

  for (const turn of mainConversationTurns(question)) {
    const processAnchors = subagentProcessAnchors(turn);
    for (const part of question.parts ?? []) {
      if (part.turn_id !== turn.id) continue;
      if (!isSubagentAnchorPart(part)) continue;
      if (seenPartIds.has(part.id)) continue;
      const portal = buildPortalFromPart({
        locator: nodeLocatorByPartId.get(part.id) ?? "",
        part,
        processAnchors,
        questionId: question.question.id,
        turn,
      });
      seenPartIds.add(part.id);
      portals.push({
        ...portal,
        turnId: turn.id,
        partIndex: part.part_index,
        turnIndex: turn.turn_index ?? 0,
      });
    }
  }

  // 零件表缺失（历史投影数据）时退回投影节点正文里的 JSON 载荷。
  if (portals.length === 0) {
    for (const node of question.projected_content_nodes ?? []) {
      if (!isSubagentAnchorNode(node)) continue;
      if (seenPartIds.has(node.part_id)) continue;
      const turn = question.turns.find((item) => item.id === node.turn_id);
      if (!turn || isLegacyPseudoTurn(turn)) continue;
      const payload = parseJsonObject(node.content);
      const childSessionId = stringField(payload?.child_session_id) ?? "";
      seenPartIds.add(node.part_id);
      portals.push({
        agentRole: stringField(payload?.agent_role),
        childSessionId,
        locator: contentNodeLocator(node),
        model: stringField(payload?.model),
        partId: node.part_id,
        partIndex: node.part_order,
        reportSummary: stringField(payload?.report_summary),
        status: normalizePortalStatus(
          stringField(payload?.status) ?? node.status,
        ),
        stepCount: numberField(payload?.step_count),
        title: stringField(payload?.title) ?? stringField(payload?.task) ?? "",
        turnId: node.turn_id,
        turnIndex: turn.turn_index ?? 0,
        unresolved: !childSessionId,
      });
    }
  }

  return portals
    .sort(
      (left, right) =>
        left.turnIndex - right.turnIndex ||
        left.partIndex - right.partIndex ||
        left.partId.localeCompare(right.partId),
    )
    .map(
      ({ partIndex: _partIndex, turnIndex: _turnIndex, ...portal }) => portal,
    );
}

/** 锚点 Part 判定：kind、卡片语义角色或渲染器任一命中即可。 */
export function isSubagentAnchorPart(part: ConversationPart): boolean {
  if ((part.kind ?? "").trim().toLowerCase() === "subagent") return true;
  const card = part.content_card;
  if (card) {
    if ((card.semantic_role ?? "").trim().toLowerCase() === "subagent")
      return true;
    if ((card.renderer ?? "").trim().toLowerCase() === "subagent_tree")
      return true;
    if ((card.kind ?? "").trim().toLowerCase().includes("subagent"))
      return true;
  }
  const metadata = parseJsonObject(part.metadata_json);
  return (
    (stringField(metadata?.type) ?? "").trim().toLowerCase() === "subagent"
  );
}

function isSubagentAnchorNode(node: ConversationContentNode): boolean {
  if ((node.semantic_role ?? "").trim().toLowerCase() === "subagent")
    return true;
  if ((node.renderer ?? "").trim().toLowerCase() === "subagent_tree")
    return true;
  return (node.node_type ?? "").trim().toLowerCase().includes("subagent");
}

function buildPortalFromPart({
  locator,
  part,
  processAnchors,
  questionId,
  turn,
}: {
  locator: string;
  part: ConversationPart;
  processAnchors: ProcessAnchorIndex;
  questionId: string;
  turn: ConversationTurn;
}): SubagentDispatchPortalViewModel {
  const metadata = parseJsonObject(part.metadata_json);
  const payload = parseJsonObject(part.text);
  const childSessionId =
    stringField(metadata?.child_session_id) ??
    stringField(payload?.child_session_id) ??
    "";
  // 子会话主键是唯一可靠的锚点身份：存量折算产物里 Part 与 ProcessItem
  // 各占一个序号（part@21 / item@22），按序号兜底会把相邻子代理串成自己。
  const processAnchor = childSessionId
    ? processAnchors.get(childSessionId)
    : undefined;

  const title =
    stringField(metadata?.title) ??
    stringField(payload?.title) ??
    stringField(payload?.task) ??
    stringField(processAnchor?.label) ??
    stringField(turn.title) ??
    "";

  return {
    agentRole:
      stringField(metadata?.agent_role) ??
      stringField(payload?.agent_role) ??
      stringField(payload?.type_name),
    childSessionId,
    locator: locator || `${questionId}:${turn.id}:${part.id}:0`,
    model: stringField(metadata?.model) ?? stringField(payload?.model),
    partId: part.id,
    reportSummary:
      stringField(metadata?.report_summary) ??
      stringField(processAnchor?.summary) ??
      stringField(payload?.report_summary),
    status: normalizePortalStatus(
      stringField(part.status) ??
        stringField(metadata?.status) ??
        stringField(processAnchor?.status) ??
        stringField(payload?.status),
    ),
    stepCount:
      numberField(metadata?.step_count) ?? numberField(payload?.step_count),
    title,
    unresolved: !childSessionId,
  };
}

interface ProcessAnchorItem {
  seq: number;
  kind: string;
  content?: string | null;
  label?: string | null;
  status?: string | null;
  summary?: string | null;
}

type ProcessAnchorIndex = Map<string, ProcessAnchorItem>;

/**
 * 解析 `turn.process` 里的子代理锚点，按子会话主键建索引。
 *
 * 序号**不能**参与配对：存量折算产物让 Part 与 ProcessItem 各占一个位置
 * （part@21 / item@22），用 `part_index` 去查 `seq` 会整体错位一格，
 * 甚至把相邻的子代理串成当前这一条。
 */
function subagentProcessAnchors(turn: ConversationTurn): ProcessAnchorIndex {
  const byChildSessionId: ProcessAnchorIndex = new Map();
  const process = parseJsonObject(turn.process);
  const items = Array.isArray(process?.items) ? process.items : [];
  for (const item of items) {
    if (!item || typeof item !== "object") continue;
    const record = item as JsonObject;
    if (stringField(record.kind) !== "subagent") continue;
    const seq = numberField(record.seq);
    if (seq === undefined) continue;
    const childSessionId = stringField(record.content);
    if (!childSessionId) continue;
    byChildSessionId.set(childSessionId, {
      content: childSessionId,
      kind: "subagent",
      label: stringField(record.label),
      seq,
      status: stringField(record.status),
      summary: stringField(record.summary),
    });
  }
  return byChildSessionId;
}

function normalizePortalStatus(status: string | null | undefined): string {
  const normalized = (status ?? "").trim().toLowerCase();
  if (!normalized) return "completed";
  if (normalized === "success" || normalized === "done") return "completed";
  if (normalized === "running" || normalized === "in_progress")
    return "running";
  if (normalized === "failed" || normalized === "error") return "failed";
  return normalized;
}

function contentNodeLocator(node: ConversationContentNode): string {
  const locator = node.locator;
  if (!locator) return node.node_id;
  return `${locator.question_id}:${locator.turn_id}:${locator.part_id}:${locator.node_order}`;
}

/**
 * 从子代理会话自身的 `execution_purpose` 反解父会话面包屑。
 *
 * 搜索命中或外部深链直接打开子会话时，没有前端跳转栈可用，
 * 只能依赖会话元数据里内联的父子溯源信息构造返回路径。
 */
export function parentSessionBreadcrumb(
  session: ConversationSession | null | undefined,
): ParentSessionBreadcrumb | null {
  if (!session) return null;
  if (
    (session.execution_origin ?? "").trim().toLowerCase() !==
    SUBAGENT_EXECUTION_ORIGIN
  )
    return null;
  const purpose = parseJsonObject(session.execution_purpose);
  const parentSessionId = stringField(purpose?.parent_session_id);
  if (!parentSessionId) return null;
  const spawnAnchor =
    purpose?.spawn_anchor && typeof purpose.spawn_anchor === "object"
      ? (purpose.spawn_anchor as JsonObject)
      : null;
  const parentTurnIndex =
    numberField(purpose?.parent_turn_index) ??
    numberField(purpose?.parent_turn) ??
    numberField(spawnAnchor?.turn_index) ??
    numberField(spawnAnchor?.parent_turn_index);
  return {
    agentRole:
      stringField(purpose?.agent_role) ?? stringField(spawnAnchor?.agent_role),
    parentSessionId,
    parentShortId: conversationIdFragment(parentSessionId),
    parentTurnIndex,
    parentTurnNumber:
      parentTurnIndex === undefined ? undefined : parentTurnIndex + 1,
  };
}

function parseJsonObject(raw: string | null | undefined): JsonObject | null {
  const trimmed = (raw ?? "").trim();
  if (!trimmed.startsWith("{")) return null;
  try {
    const parsed: unknown = JSON.parse(trimmed);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? (parsed as JsonObject)
      : null;
  } catch {
    return null;
  }
}

function stringField(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

function numberField(value: unknown): number | undefined {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim()) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : undefined;
  }
  return undefined;
}
