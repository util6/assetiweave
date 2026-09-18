import React from "react";
import { Button } from "../../ui/button";
import { ArrowLeft, Sparkles } from "lucide-react";
import { cn } from "../../../lib/utils";

interface UsageHeaderProps {
  onBack?: () => void;
  className?: string;
}

export const UsageHeader: React.FC<UsageHeaderProps> = ({
  onBack,
  className,
}) => {
  const handleBack = () => {
    if (onBack) {
      onBack();
    } else if (typeof window !== "undefined") {
      window.location.hash = "#/conversations/sessions";
    }
  };

  return (
    <div className={cn("flex items-start justify-between gap-4", className)}>
      <div className="flex flex-col gap-1.5">
        <div className="flex items-center gap-2.5">
          <div className="flex h-8 w-8 items-center justify-center rounded-xl bg-primary/10 text-primary shadow-xs">
            <Sparkles className="h-4 w-4" />
          </div>
          <h1 className="text-2xl font-bold tracking-tight text-on-surface">
            会话用量
          </h1>
        </div>
        <p className="text-body-sm text-on-surface-variant">
          从本地 Codex、Antigravity 会话原始记录提取 Token
          用量，不作为官方用量与 API 账单凭证使用。
        </p>
      </div>

      <Button
        variant="outline"
        size="sm"
        onClick={handleBack}
        className="h-9 gap-1.5 rounded-xl px-3.5 text-body-sm cursor-pointer shadow-xs border-theme-border/70 hover:bg-theme-control/50"
      >
        <ArrowLeft className="h-3.5 w-3.5" />
        <span>返回</span>
      </Button>
    </div>
  );
};
