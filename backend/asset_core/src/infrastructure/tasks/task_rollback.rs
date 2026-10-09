use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

/// 物理副作用回滚守卫 (Transactional Rollback Guard - ADR-0017 / Issue #79)
///
/// 针对文件系统、软链接或外部状态变更的事务性补偿守卫：
/// 1. 业务在执行中通过 `register` 或 `register_file_removal` 登记每一步产生的副作用；
/// 2. 若业务顺利完成，显式调用 `guard.commit()`，守卫解除，副作用永久保留；
/// 3. 若业务中途由于取消、错误提前返回 (`?`) 或触发 Panic，守卫在离开作用域 (Drop) 时
///    将按逆序 (LIFO) 自动执行所有已登记的回滚动作，清理半成品文件，杜绝残留孤立项。
pub struct RollbackGuard {
    actions: Vec<Box<dyn FnOnce() + Send + 'static>>,
    committed: bool,
}

impl Default for RollbackGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl RollbackGuard {
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
            committed: false,
        }
    }

    /// 登记任意自定义补偿闭包（例如清理缓存、撤销状态标记）
    pub fn register(&mut self, action: impl FnOnce() + Send + 'static) {
        if !self.committed {
            self.actions.push(Box::new(action));
        }
    }

    /// 便捷登记文件或软链接的物理清除
    pub fn register_file_removal(&mut self, path: impl Into<PathBuf>) {
        let path = path.into();
        self.register(move || {
            remove_file_or_symlink_if_exists(&path);
        });
    }

    /// 便捷登记目录的清理（若为空则删除）
    pub fn register_dir_removal_if_empty(&mut self, dir_path: impl Into<PathBuf>) {
        let dir_path = dir_path.into();
        self.register(move || {
            if dir_path.exists() && dir_path.is_dir() {
                if let Ok(mut entries) = std::fs::read_dir(&dir_path) {
                    if entries.next().is_none() {
                        let _ = std::fs::remove_dir(&dir_path);
                    }
                }
            }
        });
    }

    /// 标记业务操作已圆满完成，解除回滚守卫（Drop 时不再执行任何撤销动作）
    pub fn commit(&mut self) {
        self.committed = true;
        self.actions.clear();
    }

    /// 是否已经提交
    pub fn is_committed(&self) -> bool {
        self.committed
    }

    /// 已登记的回滚动作数量
    pub fn pending_count(&self) -> usize {
        self.actions.len()
    }

    /// 手动即时触发回滚并清空队列
    pub fn rollback_now(&mut self) {
        if self.committed {
            return;
        }
        for action in self.actions.drain(..).rev() {
            let _ = catch_unwind(AssertUnwindSafe(action));
        }
    }
}

impl Drop for RollbackGuard {
    fn drop(&mut self) {
        if !self.committed {
            for action in self.actions.drain(..).rev() {
                let _ = catch_unwind(AssertUnwindSafe(action));
            }
        }
    }
}

fn remove_file_or_symlink_if_exists(path: &Path) {
    if path.symlink_metadata().is_ok() {
        if let Err(err) = std::fs::remove_file(path) {
            tracing::warn!(
                target: "task_rollback",
                path = %path.display(),
                error = %err,
                "回滚清理文件或软链接失败"
            );
        } else {
            tracing::debug!(
                target: "task_rollback",
                path = %path.display(),
                "已成功回滚清理文件或软链接"
            );
        }
    }
}

#[cfg(test)]
#[path = "task_rollback_tests.rs"]
mod tests;
