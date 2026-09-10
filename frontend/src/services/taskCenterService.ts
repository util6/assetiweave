import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  TaskView,
  TaskListParams,
  TaskGetParams,
  TaskCancelParams,
  TaskRetryParams,
  TaskClearParams,
} from "../types/taskCenter";

export function isTauriRuntime(): boolean {
  return (
    typeof window !== "undefined" &&
    ("__TAURI_INTERNALS__" in window || "__TAURI__" in window)
  );
}

export async function listPublicTasks(
  params: TaskListParams = {},
): Promise<TaskView[]> {
  if (!isTauriRuntime()) return [];
  return invoke<TaskView[]>("list_public_tasks", { params });
}

export async function getPublicTask(
  taskId: string,
): Promise<TaskView | null> {
  if (!isTauriRuntime()) return null;
  const params: TaskGetParams = { task_id: taskId };
  return invoke<TaskView | null>("get_public_task", { params });
}

export async function cancelPublicTask(
  taskId: string,
): Promise<TaskView> {
  if (!isTauriRuntime()) {
    throw new Error("Task cancellation requires desktop runtime");
  }
  const params: TaskCancelParams = { task_id: taskId };
  return invoke<TaskView>("cancel_public_task", { params });
}

export async function retryPublicTask(
  taskId: string,
): Promise<TaskView> {
  if (!isTauriRuntime()) {
    throw new Error("Task retry requires desktop runtime");
  }
  const params: TaskRetryParams = { task_id: taskId };
  return invoke<TaskView>("retry_public_task", { params });
}

export async function clearTerminalTasks(
  params: TaskClearParams = {},
): Promise<number> {
  if (!isTauriRuntime()) return 0;
  return invoke<number>("clear_terminal_tasks", { params });
}

export function subscribeTaskUpdated(
  callback: (task: TaskView) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) {
    return Promise.resolve(() => undefined);
  }
  return listen<TaskView>("task-updated", (event) => {
    callback(event.payload);
  });
}
