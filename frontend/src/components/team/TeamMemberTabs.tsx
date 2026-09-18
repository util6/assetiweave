import { Shield } from "lucide-react";
import React, { useRef } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import type { TeamMember, TeamMemberSessionProjection } from "../../types/team";

export interface TeamMemberTabsProps {
  members: TeamMember[];
  activeMemberId: string | null;
  onSelectMember: (memberId: string) => void;
  getMemberProjection: (memberId: string) => TeamMemberSessionProjection | null;
  getMemberStatus: (
    projection: TeamMemberSessionProjection | null,
    t: ReturnType<typeof useI18n>["t"],
  ) => {
    label: string;
    icon: React.ReactNode;
    className: string;
  };
  roleLabel: (member: TeamMember, t: ReturnType<typeof useI18n>["t"]) => string;
}

export function TeamMemberTabs({
  activeMemberId,
  getMemberProjection,
  getMemberStatus,
  members,
  onSelectMember,
  roleLabel,
}: TeamMemberTabsProps) {
  const { t } = useI18n();
  const buttonRefs = useRef<Record<string, HTMLButtonElement | null>>({});

  const handleKeyDown = (
    event: React.KeyboardEvent<HTMLButtonElement>,
    currentIndex: number,
  ) => {
    let nextIndex: number | null = null;

    if (event.key === "ArrowRight" || event.key === "ArrowDown") {
      event.preventDefault();
      nextIndex = (currentIndex + 1) % members.length;
    } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
      event.preventDefault();
      nextIndex = (currentIndex - 1 + members.length) % members.length;
    } else if (event.key === "Home") {
      event.preventDefault();
      nextIndex = 0;
    } else if (event.key === "End") {
      event.preventDefault();
      nextIndex = members.length - 1;
    }

    if (nextIndex !== null) {
      const nextMember = members[nextIndex];
      if (nextMember) {
        onSelectMember(nextMember.id);
        buttonRefs.current[nextMember.id]?.focus();
      }
    }
  };

  return (
    <div
      aria-label={
        t("team.chat.memberNavigation") || "Member Session navigation"
      }
      className="flex min-w-0 gap-2 overflow-x-auto pb-1"
      role="tablist"
    >
      {members.map((member, index) => {
        const projection = getMemberProjection(member.id);
        const status = getMemberStatus(projection, t);
        const selected = member.id === activeMemberId;

        return (
          <button
            key={member.id}
            ref={(el) => {
              buttonRefs.current[member.id] = el;
            }}
            aria-controls={`team-member-lane-${member.id}`}
            aria-label={`${roleLabel(member, t)} · ${member.agent_id}`}
            aria-selected={selected}
            className={`group flex min-w-44 shrink-0 items-center gap-2 rounded-xl border px-2.5 py-2 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/55 ${
              selected
                ? "border-theme-nav-active-border bg-theme-nav-active/20"
                : "border-theme-card-border/65 bg-theme-card/40 hover:border-theme-nav-active-border/55 hover:bg-theme-control-hover/60"
            }`}
            data-testid={`team-member-${member.id}`}
            id={`team-member-tab-${member.id}`}
            onClick={() => onSelectMember(member.id)}
            onKeyDown={(e) => handleKeyDown(e, index)}
            role="tab"
            tabIndex={selected ? 0 : -1}
            type="button"
          >
            <span
              className={`grid size-9 shrink-0 place-items-center rounded-full border text-caption font-bold ${
                selected
                  ? "border-primary/50 bg-theme-nav-active text-theme-nav-active-fg"
                  : "border-theme-control-border bg-theme-control text-on-surface-variant"
              }`}
            >
              {member.agent_id.slice(0, 2).toUpperCase()}
            </span>
            <span className="min-w-0 flex-1">
              <span className="flex items-center gap-1.5">
                <span className="truncate text-body-sm font-semibold text-on-surface">
                  {roleLabel(member, t)}
                </span>
                {member.role === "leader" ? (
                  <Shield
                    aria-label={t("team.chat.leaderBadge") || "Team owner"}
                    className="shrink-0 text-primary"
                    size={13}
                  />
                ) : null}
              </span>
              <span className="block truncate text-caption text-on-surface-variant">
                {member.agent_id}
                {member.model ? ` · ${member.model}` : ""}
              </span>
              <span
                className={`mt-0.5 flex items-center gap-1 text-caption ${status.className}`}
                data-testid={`team-member-${member.id}-status`}
              >
                {status.icon}
                <span>{status.label}</span>
              </span>
            </span>
            {projection?.unread ? (
              <span
                aria-label={t("team.chat.unread") || "unread"}
                className="size-2 shrink-0 rounded-full bg-primary"
              />
            ) : null}
          </button>
        );
      })}
    </div>
  );
}
