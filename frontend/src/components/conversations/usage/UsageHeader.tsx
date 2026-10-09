import React from "react";
import { Button } from "../../ui/button";
import { ArrowLeft, Sparkles, Coins } from "lucide-react";
import { PageHeader } from "../../foundation/PageHeader";
import { useAppUiStore } from "../../../store/ui/appUiStore";

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

  const handleOpenPricingSettings = () => {
    useAppUiStore.getState().openSettings("conversations.pricing");
  };

  return (
    <PageHeader
      className={className}
      icon={<Sparkles className="size-3.5" />}
      eyebrow="会话用量分析"
      title="会话用量"
      description="从本地 Codex、Antigravity 会话原始记录提取 Token 用量，不作为官方用量与 API 账单凭证使用。"
      actions={
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="default"
            onClick={handleOpenPricingSettings}
            className="h-10 gap-1.5 rounded-xl border border-theme-control-border/80 bg-theme-control/70 px-3.5 text-body-sm text-theme-control-fg shadow-xs backdrop-blur-md transition-all hover:border-theme-nav-active-border/60 hover:text-on-surface"
          >
            <Coins className="size-4 text-primary" />
            <span>模型资费设置</span>
          </Button>

          <Button
            variant="outline"
            size="default"
            onClick={handleBack}
            className="h-10 gap-1.5 rounded-xl border border-theme-control-border/80 bg-theme-control/70 px-3.5 text-body-sm text-theme-control-fg shadow-xs backdrop-blur-md transition-all hover:border-theme-nav-active-border/60 hover:text-on-surface"
          >
            <ArrowLeft className="size-4" />
            <span>返回</span>
          </Button>
        </div>
      }
    />
  );
};

