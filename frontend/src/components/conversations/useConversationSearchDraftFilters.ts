import { useCallback, useEffect, useMemo, useState } from "react";

export interface ConversationSearchFilterValues {
  adapterId: string | null;
  cardKinds: string[];
  includeQuestions: boolean;
  semanticRoles: string[];
}

export interface UseConversationSearchDraftFiltersOptions {
  committedAdapterId?: string | null;
  committedCardKinds: string[];
  committedIncludeQuestions: boolean;
  committedSemanticRoles: string[];
  onAdapterChange?: (adapterId: string | null) => void;
  onApplyFilters?: (filters: ConversationSearchFilterValues) => void;
  onCardKindToggle?: (kind: string) => void;
  onQuestionToggle?: () => void;
  onResetFilters?: () => void;
  onSemanticRoleToggle?: (role: string) => void;
  open?: boolean;
}

function areSetsEqual(a: readonly string[], b: readonly string[]): boolean {
  if (a.length !== b.length) return false;
  const setA = new Set(a);
  return b.every((item) => setA.has(item));
}

export function useConversationSearchDraftFilters({
  committedAdapterId = null,
  committedCardKinds,
  committedIncludeQuestions,
  committedSemanticRoles,
  onAdapterChange,
  onApplyFilters,
  onCardKindToggle,
  onQuestionToggle,
  onResetFilters,
  onSemanticRoleToggle,
  open = true,
}: UseConversationSearchDraftFiltersOptions) {
  const [draftAdapterId, setDraftAdapterId] = useState<string | null>(
    committedAdapterId ?? null,
  );
  const [draftCardKinds, setDraftCardKinds] =
    useState<string[]>(committedCardKinds);
  const [draftSemanticRoles, setDraftSemanticRoles] = useState<string[]>(
    committedSemanticRoles,
  );
  const [draftIncludeQuestions, setDraftIncludeQuestions] = useState<boolean>(
    committedIncludeQuestions,
  );

  // Synchronize draft state when committed props change or dialog opens
  useEffect(() => {
    if (open) {
      setDraftAdapterId(committedAdapterId ?? null);
      setDraftCardKinds(committedCardKinds);
      setDraftSemanticRoles(committedSemanticRoles);
      setDraftIncludeQuestions(committedIncludeQuestions);
    }
  }, [
    open,
    committedAdapterId,
    committedCardKinds,
    committedSemanticRoles,
    committedIncludeQuestions,
  ]);

  const handleDraftAdapterChange = useCallback((id: string | null) => {
    setDraftAdapterId(id);
  }, []);

  const handleDraftCardKindToggle = useCallback((kind: string) => {
    setDraftCardKinds((prev) =>
      prev.includes(kind) ? prev.filter((k) => k !== kind) : [...prev, kind],
    );
  }, []);

  const handleDraftSemanticRoleToggle = useCallback((role: string) => {
    setDraftSemanticRoles((prev) =>
      prev.includes(role) ? prev.filter((r) => r !== role) : [...prev, role],
    );
  }, []);

  const handleDraftQuestionToggle = useCallback(() => {
    setDraftIncludeQuestions((prev) => !prev);
  }, []);

  const handleDraftReset = useCallback(() => {
    setDraftAdapterId(null);
    setDraftCardKinds([]);
    setDraftSemanticRoles([]);
    setDraftIncludeQuestions(true);
  }, []);

  const handleDiscardDraft = useCallback(() => {
    setDraftAdapterId(committedAdapterId ?? null);
    setDraftCardKinds(committedCardKinds);
    setDraftSemanticRoles(committedSemanticRoles);
    setDraftIncludeQuestions(committedIncludeQuestions);
  }, [
    committedAdapterId,
    committedCardKinds,
    committedSemanticRoles,
    committedIncludeQuestions,
  ]);

  // Compute pending differences between draft and committed state
  const hasPendingFilterChanges = useMemo(() => {
    if (draftAdapterId !== (committedAdapterId ?? null)) return true;
    if (draftIncludeQuestions !== committedIncludeQuestions) return true;
    if (!areSetsEqual(draftCardKinds, committedCardKinds)) return true;
    if (!areSetsEqual(draftSemanticRoles, committedSemanticRoles)) return true;
    return false;
  }, [
    draftAdapterId,
    committedAdapterId,
    draftIncludeQuestions,
    committedIncludeQuestions,
    draftCardKinds,
    committedCardKinds,
    draftSemanticRoles,
    committedSemanticRoles,
  ]);

  const pendingChangeCount = useMemo(() => {
    let count = 0;
    if (draftAdapterId !== (committedAdapterId ?? null)) count += 1;
    if (draftIncludeQuestions !== committedIncludeQuestions) count += 1;
    const addedKinds = draftCardKinds.filter(
      (k) => !committedCardKinds.includes(k),
    );
    const removedKinds = committedCardKinds.filter(
      (k) => !draftCardKinds.includes(k),
    );
    count += addedKinds.length + removedKinds.length;
    const addedRoles = draftSemanticRoles.filter(
      (r) => !committedSemanticRoles.includes(r),
    );
    const removedRoles = committedSemanticRoles.filter(
      (r) => !draftSemanticRoles.includes(r),
    );
    count += addedRoles.length + removedRoles.length;
    return count;
  }, [
    draftAdapterId,
    committedAdapterId,
    draftIncludeQuestions,
    committedIncludeQuestions,
    draftCardKinds,
    committedCardKinds,
    draftSemanticRoles,
    committedSemanticRoles,
  ]);

  const applyDraftFilters = useCallback(() => {
    const nextFilters: ConversationSearchFilterValues = {
      adapterId: draftAdapterId,
      cardKinds: draftCardKinds,
      includeQuestions: draftIncludeQuestions,
      semanticRoles: draftSemanticRoles,
    };

    if (onApplyFilters) {
      onApplyFilters(nextFilters);
    } else {
      // Fallback compatibility for discrete legacy callbacks
      if (draftAdapterId !== (committedAdapterId ?? null)) {
        onAdapterChange?.(draftAdapterId);
      }
      if (draftIncludeQuestions !== committedIncludeQuestions) {
        onQuestionToggle?.();
      }
      for (const kind of draftCardKinds) {
        if (!committedCardKinds.includes(kind)) {
          onCardKindToggle?.(kind);
        }
      }
      for (const kind of committedCardKinds) {
        if (!draftCardKinds.includes(kind)) {
          onCardKindToggle?.(kind);
        }
      }
      for (const role of draftSemanticRoles) {
        if (!committedSemanticRoles.includes(role)) {
          onSemanticRoleToggle?.(role);
        }
      }
      for (const role of committedSemanticRoles) {
        if (!draftSemanticRoles.includes(role)) {
          onSemanticRoleToggle?.(role);
        }
      }
    }
  }, [
    draftAdapterId,
    draftCardKinds,
    draftIncludeQuestions,
    draftSemanticRoles,
    onApplyFilters,
    committedAdapterId,
    committedIncludeQuestions,
    committedCardKinds,
    committedSemanticRoles,
    onAdapterChange,
    onQuestionToggle,
    onCardKindToggle,
    onSemanticRoleToggle,
  ]);

  const isDraftAnyFilterActive =
    Boolean(draftAdapterId) ||
    draftCardKinds.length > 0 ||
    draftSemanticRoles.length > 0 ||
    !draftIncludeQuestions;

  const isDraftAllSelected =
    draftIncludeQuestions &&
    draftCardKinds.length === 0 &&
    draftSemanticRoles.length === 0 &&
    !draftAdapterId;

  return {
    applyDraftFilters,
    draftAdapterId,
    draftCardKinds,
    draftIncludeQuestions,
    draftSemanticRoles,
    handleDiscardDraft,
    handleDraftAdapterChange,
    handleDraftCardKindToggle,
    handleDraftQuestionToggle,
    handleDraftReset,
    handleDraftSemanticRoleToggle,
    hasPendingFilterChanges,
    isDraftAllSelected,
    isDraftAnyFilterActive,
    pendingChangeCount,
  };
}
