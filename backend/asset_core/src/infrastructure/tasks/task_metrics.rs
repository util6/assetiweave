use super::task_models::{TaskKind, TaskSnapshot, TaskState};
use chrono::DateTime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

pub const TASK_METRICS_WINDOW_SIZE: usize = 100;
const RECENT_ERROR_LIMIT: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskMetricSample {
    pub kind: TaskKind,
    pub state: TaskState,
    pub duration_ms: u64,
    pub error_code: Option<String>,
    pub finished_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskKindMetricsSnapshot {
    pub kind: TaskKind,
    pub sample_count: usize,
    pub succeeded_count: u64,
    pub failed_count: u64,
    pub canceled_count: u64,
    pub p50_duration_ms: u64,
    pub p95_duration_ms: u64,
    pub recent_error_codes: Vec<String>,
}

/// 纯内存滑动窗口运行态观测器 (Issue #188)
///
/// 按 `TaskKind` 保留最近 `TASK_METRICS_WINDOW_SIZE`（100）次终态执行样本，
/// 统计成功率、P50/P95 耗时与近期错误码，完全驻留内存、零 SQLite 写盘开销。
#[derive(Debug, Default)]
pub struct TaskRuntimeMetrics {
    per_kind: Mutex<HashMap<TaskKind, VecDeque<TaskMetricSample>>>,
}

impl TaskRuntimeMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&self, snapshot: &TaskSnapshot) -> Option<u64> {
        if !snapshot.state.is_terminal() {
            return None;
        }
        let duration_ms = compute_duration_ms(snapshot);
        let sample = TaskMetricSample {
            kind: snapshot.kind,
            state: snapshot.state,
            duration_ms,
            error_code: snapshot.error.as_ref().map(|e| e.code.clone()),
            finished_at: snapshot
                .finished_at
                .clone()
                .unwrap_or_else(|| snapshot.updated_at.clone()),
        };

        let mut guard = match self.per_kind.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let window = guard.entry(snapshot.kind).or_default();
        if window.len() >= TASK_METRICS_WINDOW_SIZE {
            window.pop_front();
        }
        window.push_back(sample);
        Some(duration_ms)
    }

    pub fn snapshot(&self) -> Vec<TaskKindMetricsSnapshot> {
        let guard = match self.per_kind.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let mut results = Vec::with_capacity(guard.len());
        for (&kind, samples) in guard.iter() {
            if samples.is_empty() {
                continue;
            }
            let mut durations: Vec<u64> = samples.iter().map(|s| s.duration_ms).collect();
            durations.sort_unstable();

            let mut succeeded_count = 0u64;
            let mut failed_count = 0u64;
            let mut canceled_count = 0u64;
            let mut recent_error_codes = Vec::new();

            for sample in samples.iter().rev() {
                match sample.state {
                    TaskState::Succeeded => succeeded_count += 1,
                    TaskState::Failed => {
                        failed_count += 1;
                        if let Some(code) = &sample.error_code {
                            if recent_error_codes.len() < RECENT_ERROR_LIMIT
                                && !recent_error_codes.iter().any(|existing| existing == code)
                            {
                                recent_error_codes.push(code.clone());
                            }
                        }
                    }
                    TaskState::Canceled => canceled_count += 1,
                    _ => {}
                }
            }

            results.push(TaskKindMetricsSnapshot {
                kind,
                sample_count: samples.len(),
                succeeded_count,
                failed_count,
                canceled_count,
                p50_duration_ms: percentile(&durations, 50),
                p95_duration_ms: percentile(&durations, 95),
                recent_error_codes,
            });
        }
        results.sort_by_key(|item| item.kind.to_string());
        results
    }
}

fn compute_duration_ms(snapshot: &TaskSnapshot) -> u64 {
    let Some(finished_raw) = snapshot.finished_at.as_deref() else {
        return 0;
    };
    let Ok(started) = DateTime::parse_from_rfc3339(&snapshot.started_at) else {
        return 0;
    };
    let Ok(finished) = DateTime::parse_from_rfc3339(finished_raw) else {
        return 0;
    };
    let delta = finished.signed_duration_since(started).num_milliseconds();
    u64::try_from(delta.max(0)).unwrap_or(0)
}

fn percentile(sorted: &[u64], pct: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() * pct).div_ceil(100))
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[idx]
}
