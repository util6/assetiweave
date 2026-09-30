import { useState, useEffect } from "react";
import { Users2, Plus, Trash2, Crown, User, Sparkles } from "lucide-react";
import { DialogFrame } from "../../foundation/DialogFrame";
import { Button } from "../../ui/button";
import { Input } from "../../ui/input";
import { Badge } from "../../foundation/Badge";
import {
  SUPPORTED_AGENTS,
  createTeam,
} from "../../../services/agentTeamService";
import {
  loadStoredAccounts,
  subscribeAgentAccounts,
} from "../../../services/agentAccountService";
import type {
  TeamDefinition,
  TeamMemberConfig,
  AgentAccount,
} from "../../../types/agentWorkspace";

export interface TeamCreateModalProps {
  isOpen: boolean;
  onClose: () => void;
  onTeamCreated: (team: TeamDefinition) => void;
}

export function TeamCreateModal({
  isOpen,
  onClose,
  onTeamCreated,
}: TeamCreateModalProps) {
  const [teamName, setTeamName] = useState("");
  const [teamDesc, setTeamDesc] = useState("");
  const [accounts, setAccounts] = useState<AgentAccount[]>(loadStoredAccounts);

  useEffect(() => {
    return subscribeAgentAccounts((updated) => {
      setAccounts(updated);
    });
  }, []);

  const [members, setMembers] = useState<TeamMemberConfig[]>([
    {
      memberId: "m_leader_1",
      name: "总指挥架构师",
      role: "leader",
      agentId: "codex",
      model: "o3-mini",
      accountId: "acc_codex_work",
    },
    {
      memberId: "m_teammate_2",
      name: "核心开发员",
      role: "teammate",
      agentId: "antigravity",
      model: "gemini-2.5-pro",
      accountId: "acc_agy_pro",
    },
  ]);

  if (!isOpen) return null;

  const handleAddMember = () => {
    const codexAccs = accounts.filter((a) => a.provider === "codex");
    const newMember: TeamMemberConfig = {
      memberId: `m_teammate_${Date.now()}`,
      name: `协同成员 ${members.length + 1}`,
      role: "teammate",
      agentId: "codex",
      model: "o3-mini",
      accountId: codexAccs[0]?.id ?? "",
    };
    setMembers([...members, newMember]);
  };

  const handleUpdateMember = (
    index: number,
    updates: Partial<TeamMemberConfig>,
  ) => {
    const updated = [...members];
    const target = { ...updated[index], ...updates };

    // If agentId changed, reset model and accountId
    if (updates.agentId && updates.agentId !== updated[index].agentId) {
      const agent =
        SUPPORTED_AGENTS.find((a) => a.id === updates.agentId) ??
        SUPPORTED_AGENTS[0];
      target.model = agent.defaultModel;
      const matchingAccs = accounts.filter(
        (a) => a.provider === agent.provider,
      );
      target.accountId = matchingAccs[0]?.id ?? "";
    }

    updated[index] = target;
    setMembers(updated);
  };

  const handleRemoveMember = (index: number) => {
    if (members[index].role === "leader") return;
    setMembers(members.filter((_, i) => i !== index));
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!teamName.trim() || members.length === 0) return;

    const leader = members.find((m) => m.role === "leader") ?? members[0];

    const created = await createTeam({
      name: teamName.trim(),
      description: teamDesc.trim() || undefined,
      leaderMemberId: leader.memberId,
      members,
    });

    onTeamCreated(created);
    onClose();
  };

  return (
    <DialogFrame
      layer="default"
      onClose={onClose}
      size="xl"
      icon={<Users2 className="h-5 w-5" />}
      title="创建协同团队与成员配额绑定"
      description="为每个团队成员独立指定智能体角色、模型与账号 Profile，实现并行执行时的配额完全隔离"
    >
      <form onSubmit={handleSubmit} className="space-y-4">
        {/* Team Meta */}
        <div className="grid grid-cols-3 gap-3">
          <div className="col-span-1">
            <label className="mb-1 block text-body-xs font-medium text-theme-text">
              团队名称 *
            </label>
            <Input
              placeholder="例如: 全栈敏捷重构组"
              value={teamName}
              onChange={(e) => setTeamName(e.target.value)}
              required
            />
          </div>
          <div className="col-span-2">
            <label className="mb-1 block text-body-xs font-medium text-theme-text">
              目标与职责说明 (可选)
            </label>
            <Input
              placeholder="简述团队的核心协同任务、分工和交付标准..."
              value={teamDesc}
              onChange={(e) => setTeamDesc(e.target.value)}
            />
          </div>
        </div>

        {/* Members Roster Table */}
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <div className="text-body-xs font-semibold text-theme-text uppercase tracking-wider">
              团队成员编排清单 ({members.length})
            </div>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={handleAddMember}
              className="gap-1.5"
            >
              <Plus className="h-3.5 w-3.5" />
              <span>添加成员</span>
            </Button>
          </div>

          <div className="max-h-[320px] space-y-2.5 overflow-y-auto pr-1">
            {members.map((member, idx) => {
              const currentAgentDef =
                SUPPORTED_AGENTS.find((a) => a.id === member.agentId) ??
                SUPPORTED_AGENTS[0];
              const matchingAccounts = accounts.filter(
                (a) => a.provider === currentAgentDef.provider,
              );

              return (
                <div
                  key={member.memberId}
                  className="grid grid-cols-12 items-center gap-2.5 rounded-xl border border-theme-border/50 bg-theme-panel/35 p-3 transition-colors hover:border-theme-border/80 hover:bg-theme-panel/55"
                >
                  {/* Role & Name */}
                  <div className="col-span-3">
                    <div className="flex items-center gap-1.5 mb-1">
                      {member.role === "leader" ? (
                        <span className="flex items-center gap-1 text-[11px] font-semibold text-status-warning">
                          <Crown className="h-3 w-3" />
                          <span>组长 (Leader)</span>
                        </span>
                      ) : (
                        <span className="flex items-center gap-1 text-[11px] font-medium text-theme-muted">
                          <User className="h-3 w-3" />
                          <span>成员</span>
                        </span>
                      )}
                    </div>
                    <Input
                      value={member.name}
                      onChange={(e) =>
                        handleUpdateMember(idx, { name: e.target.value })
                      }
                      placeholder="成员姓名/职务"
                      required
                    />
                  </div>

                  {/* Agent Selector */}
                  <div className="col-span-3">
                    <label className="mb-1 block text-[11px] text-theme-muted">
                      选用智能体
                    </label>
                    <select
                      aria-label="选用智能体"
                      className="w-full rounded-lg border border-theme-border/60 bg-theme-panel/90 px-2 py-1.5 text-body-xs text-theme-text focus:outline-none focus:ring-1 focus:ring-primary-strong"
                      value={member.agentId}
                      onChange={(e) =>
                        handleUpdateMember(idx, { agentId: e.target.value })
                      }
                    >
                      {SUPPORTED_AGENTS.map((agent) => (
                        <option key={agent.id} value={agent.id}>
                          {agent.displayName}
                        </option>
                      ))}
                    </select>
                  </div>

                  {/* Model Selector */}
                  <div className="col-span-2">
                    <label className="mb-1 block text-[11px] text-theme-muted">
                      模型
                    </label>
                    <select
                      aria-label="模型"
                      className="w-full rounded-lg border border-theme-border/60 bg-theme-panel/90 px-2 py-1.5 text-body-xs text-theme-text focus:outline-none focus:ring-1 focus:ring-primary-strong"
                      value={member.model}
                      onChange={(e) =>
                        handleUpdateMember(idx, { model: e.target.value })
                      }
                    >
                      {currentAgentDef.availableModels.map((m) => (
                        <option key={m.id} value={m.id}>
                          {m.label}
                        </option>
                      ))}
                    </select>
                  </div>

                  {/* Crucial Account Profile Selector (切号选择!) */}
                  <div className="col-span-3">
                    <label className="mb-1 block text-[11px] font-medium text-primary-strong">
                      绑定账号 (配额隔离) *
                    </label>
                    <select
                      aria-label="绑定账号 (配额隔离)"
                      className="w-full rounded-lg border border-primary-strong/40 bg-primary-subtle/10 px-2 py-1.5 text-body-xs font-medium text-theme-text focus:outline-none focus:ring-1 focus:ring-primary-strong"
                      value={member.accountId}
                      onChange={(e) =>
                        handleUpdateMember(idx, { accountId: e.target.value })
                      }
                    >
                      {matchingAccounts.length === 0 ? (
                        <option value="">暂无可用账号</option>
                      ) : (
                        matchingAccounts.map((acc) => (
                          <option key={acc.id} value={acc.id}>
                            {acc.displayName}{" "}
                            {acc.email ? `(${acc.email})` : ""}
                          </option>
                        ))
                      )}
                    </select>
                  </div>

                  {/* Remove Button */}
                  <div className="col-span-1 flex justify-end pt-4">
                    {member.role !== "leader" && (
                      <button
                        type="button"
                        aria-label="移除成员"
                        onClick={() => handleRemoveMember(idx)}
                        className="rounded-lg p-1.5 text-theme-muted transition-colors hover:bg-status-conflict/15 hover:text-status-conflict"
                      >
                        <Trash2 className="h-4 w-4" />
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between border-t border-theme-border/40 pt-3">
          <div className="text-body-xs text-theme-muted">
            提示：为团队成员指定不同账号可有效避免速率限制（429 Too Many
            Requests）。
          </div>
          <div className="flex gap-2">
            <Button type="button" variant="ghost" size="sm" onClick={onClose}>
              取消
            </Button>
            <Button type="submit" variant="default" size="sm">
              创建并开启团队协作
            </Button>
          </div>
        </div>
      </form>
    </DialogFrame>
  );
}
