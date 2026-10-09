use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[test]
fn test_rollback_guard_committed_does_not_rollback() {
    let counter = Arc::new(AtomicUsize::new(0));
    {
        let mut guard = RollbackGuard::new();
        let c = counter.clone();
        guard.register(move || {
            c.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(guard.pending_count(), 1);
        guard.commit();
        assert!(guard.is_committed());
    }
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[test]
fn test_rollback_guard_uncommitted_drop_executes_in_reverse_order() {
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));
    {
        let mut guard = RollbackGuard::new();
        let o1 = order.clone();
        guard.register(move || {
            o1.lock().unwrap().push(1);
        });
        let o2 = order.clone();
        guard.register(move || {
            o2.lock().unwrap().push(2);
        });
        let o3 = order.clone();
        guard.register(move || {
            o3.lock().unwrap().push(3);
        });
        // 不调用 commit，离开作用域自动 drop
    }
    // 逆序执行：3, 2, 1
    assert_eq!(*order.lock().unwrap(), vec![3, 2, 1]);
}

#[test]
fn test_rollback_guard_file_removal() {
    let test_dir = std::env::temp_dir().join(format!("aiw_rollback_test_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&test_dir);
    let test_file = test_dir.join("test_orphan.txt");
    std::fs::write(&test_file, "temporary content").expect("write test file");
    assert!(test_file.exists());

    {
        let mut guard = RollbackGuard::new();
        // 现实业务中先创建目录，后在目录中创建文件；逆序回滚时先删文件，再删空目录
        guard.register_dir_removal_if_empty(&test_dir);
        guard.register_file_removal(&test_file);
        // 模拟提前错误退出
    }

    // 守卫析构后，文件和空目录应已被清理
    assert!(!test_file.exists());
    assert!(!test_dir.exists());
}

#[test]
fn test_rollback_guard_panic_resilient() {
    let second_executed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut guard = RollbackGuard::new();
        let flag = second_executed.clone();
        guard.register(move || {
            flag.store(true, Ordering::SeqCst);
        });
        guard.register(|| {
            panic!("回滚闭包内部意外崩溃");
        });
    }
    // 即使前面的闭包 panic，后面的闭包依然会被正常执行
    assert!(second_executed.load(Ordering::SeqCst));
}
