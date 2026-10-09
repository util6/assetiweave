import type { QueryClient } from "@tanstack/react-query";
import type { TaskView } from "../../types/taskCenter";

export type InvalidationCallback = (
  task: TaskView,
  queryClient: QueryClient,
) => void | Promise<void>;

/**
 * 领域自注册的查询缓存失效中心 (ADR-0017)
 * 业务领域按需自注册其关心的 task kind / category，在收到终态 Structural 事件时自动失效对应缓存。
 */
class TaskInvalidationRegistry {
  private handlers = new Map<string, Set<InvalidationCallback>>();

  /**
   * 注册指定 kind 或 category 的失效回调（大小写不敏感）
   * 返回注销函数
   */
  register(kindOrCategory: string, callback: InvalidationCallback): () => void {
    const key = kindOrCategory.toLowerCase();
    if (!this.handlers.has(key)) {
      this.handlers.set(key, new Set());
    }
    this.handlers.get(key)!.add(callback);

    return () => {
      const set = this.handlers.get(key);
      if (set) {
        set.delete(callback);
        if (set.size === 0) {
          this.handlers.delete(key);
        }
      }
    };
  }

  /**
   * 触发失效回调
   */
  async trigger(task: TaskView, queryClient: QueryClient): Promise<void> {
    const keys = [
      task.kind?.toLowerCase(),
      task.category?.toLowerCase(),
    ].filter((k): k is string => Boolean(k));

    const callbacks = new Set<InvalidationCallback>();
    for (const key of keys) {
      const set = this.handlers.get(key);
      if (set) {
        for (const cb of set) {
          callbacks.add(cb);
        }
      }
    }

    for (const cb of callbacks) {
      try {
        await cb(task, queryClient);
      } catch (err) {
        console.error(
          `[taskInvalidationRegistry] error running invalidation for task ${task.id}:`,
          err,
        );
      }
    }
  }
}

export const taskInvalidationRegistry = new TaskInvalidationRegistry();
