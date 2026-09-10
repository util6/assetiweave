import React, { Component, type ErrorInfo, type ReactNode } from "react";
import { AlertCircle, RotateCcw } from "lucide-react";
import { Button } from "../ui/button";

export interface ErrorBoundaryProps {
  children: ReactNode;
  fallback?: ReactNode | ((error: Error, reset: () => void) => ReactNode);
  onError?: (error: Error, errorInfo: ErrorInfo) => void;
  onReset?: () => void;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<
  ErrorBoundaryProps,
  ErrorBoundaryState
> {
  public override state: ErrorBoundaryState = {
    hasError: false,
    error: null,
  };

  public static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  public override componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error("[ErrorBoundary] Caught error:", error, errorInfo);
    this.props.onError?.(error, errorInfo);
  }

  public reset = () => {
    this.props.onReset?.();
    this.setState({ hasError: false, error: null });
  };

  public override render() {
    if (this.state.hasError && this.state.error) {
      if (typeof this.props.fallback === "function") {
        return this.props.fallback(this.state.error, this.reset);
      }
      if (this.props.fallback) {
        return this.props.fallback;
      }

      return (
        <div className="flex h-full min-h-[200px] w-full flex-col items-center justify-center gap-3 p-6 text-center">
          <div className="flex size-12 items-center justify-center rounded-2xl border border-status-conflict/30 bg-status-conflict/10 text-status-conflict">
            <AlertCircle className="size-6" />
          </div>
          <div className="flex flex-col gap-1">
            <h3 className="text-body-md font-semibold text-on-surface">
              组件渲染遇到问题
            </h3>
            <p className="max-w-md text-caption text-on-surface-variant font-mono">
              {this.state.error.message}
            </p>
          </div>
          <Button
            className="mt-2"
            onClick={this.reset}
            size="sm"
            type="button"
            variant="outline"
          >
            <RotateCcw className="mr-1.5 size-3.5" />
            重试
          </Button>
        </div>
      );
    }

    return this.props.children;
  }
}
