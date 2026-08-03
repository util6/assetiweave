import { BookOpen, Braces, Brain, CheckCircle2, FileText, FolderOpen, Terminal, Wrench, type LucideIcon } from "lucide-react";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { listConversationAdapters } from "../../services/conversations";
import type { ConversationCardKindDefinition, ConversationCardRenderer } from "../../types";

interface ConversationCardKindRegistryValue {
  definitions: ReadonlyMap<string, ConversationCardKindDefinition>;
  refresh: () => Promise<void>;
}

export const CORE_CONVERSATION_CARD_SEMANTIC_ROLES = [
  "answer",
  "tool",
  "command",
  "code",
  "result",
] as const;

const coreConversationCardSemanticRoles = new Set<string>(CORE_CONVERSATION_CARD_SEMANTIC_ROLES);

export function conversationCardPresentationKind(kind: string, semanticRole?: string | null) {
  if (semanticRole && coreConversationCardSemanticRoles.has(semanticRole)) return semanticRole;
  const suffix = kind.includes(".") ? kind.slice(kind.lastIndexOf(".") + 1) : kind;
  return kind !== suffix && coreConversationCardSemanticRoles.has(suffix) ? suffix : kind;
}

export function isRedundantConversationCardKind(
  kind: string,
  definition?: ConversationCardKindDefinition,
) {
  if (definition?.semantic_role && coreConversationCardSemanticRoles.has(definition.semantic_role)) {
    return true;
  }
  const suffix = kind.includes(".") ? kind.slice(kind.lastIndexOf(".") + 1) : kind;
  return kind !== suffix && coreConversationCardSemanticRoles.has(suffix);
}

const emptyDefinitions = new Map<string, ConversationCardKindDefinition>();
const ConversationCardKindRegistryContext = createContext<ConversationCardKindRegistryValue>({
  definitions: emptyDefinitions,
  refresh: async () => undefined,
});

export function ConversationCardKindRegistryProvider({ children }: { children: ReactNode }) {
  const [definitions, setDefinitions] = useState<ReadonlyMap<string, ConversationCardKindDefinition>>(
    emptyDefinitions,
  );
  const refresh = useCallback(async () => {
    const adapters = await listConversationAdapters();
    setDefinitions(new Map(
      adapters.flatMap((adapter) => adapter.card_kinds ?? []).map((definition) => [definition.id, definition]),
    ));
  }, []);

  useEffect(() => {
    void refresh();
    const handleRefresh = () => void refresh();
    window.addEventListener("assetiweave:conversation-adapters-changed", handleRefresh);
    return () => window.removeEventListener("assetiweave:conversation-adapters-changed", handleRefresh);
  }, [refresh]);

  const value = useMemo(() => ({ definitions, refresh }), [definitions, refresh]);
  return (
    <ConversationCardKindRegistryContext.Provider value={value}>
      {children}
    </ConversationCardKindRegistryContext.Provider>
  );
}

export function useConversationCardKindRegistry() {
  return useContext(ConversationCardKindRegistryContext);
}

const iconHints: Record<string, LucideIcon> = {
  brain: Brain,
  "book-open": BookOpen,
  code: Braces,
  command: Terminal,
  result: CheckCircle2,
  terminal: Terminal,
  tool: Wrench,
};

const rendererIcons: Record<ConversationCardRenderer, LucideIcon> = {
  markdown: FileText,
  plain: FileText,
  path: FolderOpen,
  json: Braces,
  code: Braces,
  command: Terminal,
  terminal_output: CheckCircle2,
};

const builtInKindIconHints: Record<string, keyof typeof iconHints> = {
  answer: "result",
  tool: "tool",
  command: "command",
  code: "code",
  result: "result",
};

export function ConversationCardKindIcon({
  iconHint,
  kind,
  renderer,
  size = 15,
}: {
  iconHint?: string | null;
  kind?: string;
  renderer: ConversationCardRenderer;
  size?: number;
}) {
  const Icon = (iconHint ? iconHints[iconHint] : undefined)
    ?? (kind ? iconHints[builtInKindIconHints[kind]] : undefined)
    ?? rendererIcons[renderer];
  return <Icon aria-hidden="true" size={size} />;
}
