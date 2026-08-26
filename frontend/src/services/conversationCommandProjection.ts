import { invoke } from "@tauri-apps/api/core";

const MAX_PROJECTION_BATCH_SIZE = 128;

export interface ConversationCommandProjectionPartInput {
  partId: string;
  command: string;
  commandLabel?: string | null;
}

export interface ConversationCommandProjectionNode {
  display_order: number;
  command: string;
  command_label?: string | null;
}

export interface ConversationCommandProjection {
  part_id: string;
  schema_version: number;
  projector_version: string;
  nodes: ConversationCommandProjectionNode[];
}

export interface ConversationCommandProjectionRequest {
  adapterId: string;
  adapterVersion: string;
  parts: ConversationCommandProjectionPartInput[];
}

interface ProjectionCacheEntry {
  command: string;
  projection: ConversationCommandProjection;
}

const projectionCache = new Map<string, ProjectionCacheEntry>();
const latestProjectionKeyBySource = new Map<string, string>();
const inFlightBatches = new Map<string, Promise<ConversationCommandProjection[]>>();

export async function projectConversationCommandParts(
  request: ConversationCommandProjectionRequest,
): Promise<ConversationCommandProjection[]> {
  if (request.parts.length === 0) return [];
  const normalizedParts = request.parts.map(normalizePart);
  const projections = new Map<string, ConversationCommandProjection>();
  const missing: ConversationCommandProjectionPartInput[] = [];

  for (const part of normalizedParts) {
    const sourceKey = projectionSourceKey(request, part);
    const projectionKey = latestProjectionKeyBySource.get(sourceKey);
    const cached = projectionKey ? projectionCache.get(projectionKey) : undefined;
    if (cached?.command === part.command) {
      projections.set(part.partId, cached.projection);
    } else {
      missing.push(part);
    }
  }

  for (let offset = 0; offset < missing.length; offset += MAX_PROJECTION_BATCH_SIZE) {
    const batch = missing.slice(offset, offset + MAX_PROJECTION_BATCH_SIZE);
    const projected = await projectBatch(request, batch);
    for (const projection of projected) {
      const part = batch.find((candidate) => candidate.partId === projection.part_id)!;
      const sourceKey = projectionSourceKey(request, part);
      const projectionKey = `${sourceKey}\0${projection.projector_version}`;
      projectionCache.set(projectionKey, { command: part.command, projection });
      latestProjectionKeyBySource.set(sourceKey, projectionKey);
      projections.set(part.partId, projection);
    }
  }

  return normalizedParts.map((part) => projections.get(part.partId)!);
}

async function projectBatch(
  request: ConversationCommandProjectionRequest,
  parts: ConversationCommandProjectionPartInput[],
): Promise<ConversationCommandProjection[]> {
  const batchKey = parts.map((part) => projectionSourceKey(request, part)).join("\u001e");
  const current = inFlightBatches.get(batchKey);
  if (current) return current;
  const promise = projectBatchUncached(request.adapterId, parts).finally(() => {
    inFlightBatches.delete(batchKey);
  });
  inFlightBatches.set(batchKey, promise);
  return promise;
}

async function projectBatchUncached(
  adapterId: string,
  parts: ConversationCommandProjectionPartInput[],
): Promise<ConversationCommandProjection[]> {
  if (!isTauriRuntime()) {
    return parts.map((part) => ({
      part_id: part.partId,
      schema_version: 1,
      projector_version: "browser-preview-raw-v1",
      nodes: [{
        display_order: 0,
        command: part.command,
        command_label: part.commandLabel ?? null,
      }],
    }));
  }
  const result = await invoke<ConversationCommandProjection[]>(
    "project_conversation_command_parts",
    {
      params: {
        adapter_id: adapterId,
        parts: parts.map((part) => ({
          part_id: part.partId,
          command: part.command,
          command_label: part.commandLabel ?? null,
        })),
      },
    },
  );
  return validateProjectionBatch(parts, result);
}

function validateProjectionBatch(
  parts: ConversationCommandProjectionPartInput[],
  result: ConversationCommandProjection[],
) {
  const requestedIds = new Set(parts.map((part) => part.partId));
  const seen = new Set<string>();
  for (const projection of result) {
    if (!requestedIds.has(projection.part_id) || seen.has(projection.part_id)) {
      throw new Error(`Invalid command projection Part: ${projection.part_id}`);
    }
    if (projection.schema_version !== 1 || !projection.projector_version?.trim()) {
      throw new Error(`Invalid command projection version for Part: ${projection.part_id}`);
    }
    projection.nodes.forEach((node, index) => {
      if (node.display_order !== index || !node.command?.trim()) {
        throw new Error(`Invalid command projection node for Part: ${projection.part_id}`);
      }
    });
    seen.add(projection.part_id);
  }
  if (seen.size !== parts.length) {
    throw new Error("Command projector returned an incomplete batch");
  }
  const byPartId = new Map(result.map((projection) => [projection.part_id, projection]));
  return parts.map((part) => byPartId.get(part.partId)!);
}

function normalizePart(part: ConversationCommandProjectionPartInput) {
  const partId = part.partId.trim();
  if (!partId || !part.command.trim()) {
    throw new Error("Command projection requires a Part ID and raw command");
  }
  return { ...part, partId };
}

function projectionSourceKey(
  request: Pick<ConversationCommandProjectionRequest, "adapterId" | "adapterVersion">,
  part: ConversationCommandProjectionPartInput,
) {
  return [
    request.adapterId,
    request.adapterVersion,
    part.partId,
    commandHash(part.command),
    commandHash(part.commandLabel ?? ""),
  ].join("\0");
}

function commandHash(value: string) {
  let first = 0x811c9dc5;
  let second = 0x9e3779b9;
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    first = Math.imul(first ^ code, 0x01000193);
    second = Math.imul(second ^ code, 0x85ebca6b);
  }
  return `${(first >>> 0).toString(16).padStart(8, "0")}${(second >>> 0).toString(16).padStart(8, "0")}`;
}

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function __resetConversationCommandProjectionCacheForTests() {
  projectionCache.clear();
  latestProjectionKeyBySource.clear();
  inFlightBatches.clear();
}
