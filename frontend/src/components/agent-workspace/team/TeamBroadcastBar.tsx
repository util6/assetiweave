import { useState } from "react";
import { Megaphone, Send, ArrowUpRight } from "lucide-react";
import { Button } from "../../ui/button";

export interface TeamBroadcastBarProps {
  onBroadcast: (prompt: string, mode: "all" | "leader") => void;
  disabled?: boolean;
  memberCount: number;
}

export function TeamBroadcastBar({
  onBroadcast,
  disabled = false,
  memberCount,
}: TeamBroadcastBarProps) {
  const [prompt, setPrompt] = useState("");
  const [dispatchMode, setDispatchMode] = useState<"all" | "leader">("all");

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!prompt.trim() || disabled) return;
    onBroadcast(prompt.trim(), dispatchMode);
    setPrompt("");
  };

  return (
    <div className="border-t border-theme-border/60 bg-theme-panel/70 p-3 backdrop-blur-md">
      <form
        onSubmit={handleSubmit}
        className="flex items-center gap-2 max-w-4xl mx-auto"
      >
        <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-primary-subtle text-primary-strong shadow-xs">
          <Megaphone className="h-4 w-4" />
        </div>

        <div className="flex flex-1 items-center rounded-2xl border border-theme-border/60 bg-theme-panel/90 px-3 py-1.5 focus-within:border-primary-strong/60 focus-within:ring-1 focus-within:ring-primary-strong/40 transition-all shadow-inner">
          <input
            type="text"
            className="flex-1 bg-transparent text-body-xs text-theme-text placeholder:text-theme-muted focus:outline-none"
            placeholder={`向整个团队 (${memberCount} 位智能体) 广播总目标或协作任务...`}
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            disabled={disabled}
          />

          <div className="flex items-center gap-1.5 border-l border-theme-border/40 pl-2">
            <select
              aria-label="派发模式"
              className="rounded-lg bg-transparent text-[11px] font-medium text-theme-muted hover:text-theme-text focus:outline-none cursor-pointer"
              value={dispatchMode}
              onChange={(e) =>
                setDispatchMode(e.target.value as "all" | "leader")
              }
            >
              <option value="all">全员同步下发</option>
              <option value="leader">由组长拆解分派</option>
            </select>
          </div>
        </div>

        <Button
          type="submit"
          size="sm"
          disabled={!prompt.trim() || disabled}
          className="h-9 px-4 rounded-xl gap-1.5 shadow-sm"
        >
          <span>全队下发</span>
          <Send className="h-3.5 w-3.5" />
        </Button>
      </form>
    </div>
  );
}
