use super::backend::infrastructure::tasks::{TaskChangeClass, TaskKind, TaskSnapshot, TaskState};
use super::{has_startup_self_check_arg, should_emit_task_snapshot};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[test]
fn recognizes_startup_self_check_argument_without_matching_similar_values() {
    assert!(has_startup_self_check_arg([
        "assetiweave",
        "--startup-self-check"
    ]));
    assert!(!has_startup_self_check_arg([
        "assetiweave",
        "--startup-self-check=true"
    ]));
}

#[test]
fn structural_and_terminal_task_snapshots_bypass_desktop_throttle() {
    let mut last_emitted: HashMap<String, Instant> = HashMap::new();
    let mut snap = TaskSnapshot::empty("task-1");
    snap.kind = TaskKind::ConversationSync;
    snap.user_visible = true;
    snap.state = TaskState::Running;
    let t0 = Instant::now();

    // 首帧 Cosmetic 允许放行
    snap.change_class = TaskChangeClass::Cosmetic;
    assert!(should_emit_task_snapshot(&snap, &mut last_emitted, t0));

    // 10ms 内的第二帧 Cosmetic 被节流
    let now = t0 + Duration::from_millis(10);
    assert!(!should_emit_task_snapshot(&snap, &mut last_emitted, now));

    // 但同窗口内的 Structural 帧必须无条件放行
    snap.change_class = TaskChangeClass::Structural;
    assert!(should_emit_task_snapshot(&snap, &mut last_emitted, now));

    // 终态帧也必须无条件放行并清理节流记录
    snap.state = TaskState::Succeeded;
    snap.change_class = TaskChangeClass::Structural;
    assert!(should_emit_task_snapshot(&snap, &mut last_emitted, now));
    assert!(!last_emitted.contains_key("task-1"));
}
