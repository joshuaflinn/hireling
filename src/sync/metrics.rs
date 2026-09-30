//! Dispatch metrics — E7 (plan Task 8).
//!
//! A 10 000-sample in-process ring of per-applied-write dispatch times and
//! the session-gated `/metrics/sync` summary (contract §7). The budget
//! (dispatch p95 ≤ 100 ms at POC load) is read here; the ring exists so the
//! window is bounded, not so the budget is tunable — a failing budget is a
//! stop-and-report, never a threshold edit.

use std::collections::VecDeque;
use std::sync::Mutex;

use axum::Extension;
use axum::extract::State;
use axum::response::IntoResponse;
use serde::Serialize;

use crate::auth::middleware::SessionAccount;

/// The window size (contract §7, binding).
pub const WINDOW: usize = 10_000;

/// The dispatch-time ring. One per process, owned by
/// [`SyncState`](crate::sync::SyncState).
#[derive(Default)]
pub struct SyncMetrics {
    samples: Mutex<VecDeque<f64>>,
}

/// The endpoint's answer: the window's size and its nearest-rank
/// percentiles, milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MetricsSummary {
    pub count: u64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
}

impl SyncMetrics {
    #[must_use]
    pub fn new() -> Self {
        Self {
            samples: Mutex::new(VecDeque::with_capacity(WINDOW)),
        }
    }

    /// Record one dispatch time, milliseconds. Evicts the oldest sample at
    /// the window edge.
    pub fn push(&self, ms: f64) {
        let mut samples = self.lock();
        if samples.len() >= WINDOW {
            samples.pop_front();
        }
        samples.push_back(ms);
    }

    /// The current summary.
    #[must_use]
    pub fn snapshot(&self) -> MetricsSummary {
        let samples = self.lock();
        let mut sorted: Vec<f64> = samples.iter().copied().collect();
        sorted.sort_by(f64::total_cmp);
        MetricsSummary {
            count: sorted.len() as u64,
            p50_ms: percentile(&sorted, 50),
            p95_ms: percentile(&sorted, 95),
            p99_ms: percentile(&sorted, 99),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<f64>> {
        self.samples
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Nearest-rank percentile of a sorted slice: `ceil(p/100 · n)`-th value
/// (exact integer math — `p ≤ 100`, `n ≤ WINDOW`), 0.0 for an empty window.
fn percentile(sorted: &[f64], p: u32) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return 0.0;
    }
    let rank = (p as usize * n).div_ceil(100);
    // rank is clamped into 1..=n, so the index is always in bounds — but
    // indexing lints deny, so the invariant is stated as code.
    sorted
        .get(rank.clamp(1, n) - 1)
        .copied()
        .unwrap_or(f64::NAN)
}

/// `GET /api/metrics/sync` — the summary, session-gated like every /api
/// route (contract §7). No party filter at POC: one party exists.
pub async fn sync_metrics(
    State(_auth): State<std::sync::Arc<crate::auth::AuthState>>,
    Extension(sync): Extension<crate::sync::SyncState>,
    _account: SessionAccount,
) -> impl IntoResponse {
    axum::Json(sync.metrics.snapshot())
}

/// The log label for a write's target — same shape as the ledger's
/// `field_path` (data-model.md): `vitals:hp`, `slot:Wizard:3:0`, `inv:Chalk`.
#[must_use]
pub fn field_label(target: &crate::sync::protocol::FieldTarget) -> String {
    match target {
        crate::sync::protocol::FieldTarget::Vitals { field, .. } => {
            format!("vitals:{}", field.as_str())
        }
        crate::sync::protocol::FieldTarget::Slot {
            caster_key,
            rank,
            slot_index,
            ..
        } => format!("slot:{caster_key}:{rank}:{slot_index}"),
        crate::sync::protocol::FieldTarget::Inv { item_name, .. } => {
            format!("inv:{item_name}")
        }
        crate::sync::protocol::FieldTarget::Effect { effect_id } => {
            format!("effect:{effect_id}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_field_label_matches_the_ledger_shape() {
        let label = |target| field_label(&target);
        assert_eq!(
            label(crate::sync::protocol::FieldTarget::Vitals {
                character_id: 3,
                field: crate::sync::protocol::VitalsField::Hp,
            }),
            "vitals:hp"
        );
        assert_eq!(
            label(crate::sync::protocol::FieldTarget::Slot {
                character_id: 3,
                caster_key: "Wizard".to_owned(),
                rank: 3,
                slot_index: 0,
            }),
            "slot:Wizard:3:0"
        );
        assert_eq!(
            label(crate::sync::protocol::FieldTarget::Inv {
                character_id: 3,
                item_name: "Chalk".to_owned(),
            }),
            "inv:Chalk"
        );
    }

    #[test]
    fn percentiles_clamp_inside_the_sample_set() {
        let one = [7.0_f64];
        assert!((percentile(&one, 50) - 7.0).abs() < f64::EPSILON);
        assert!((percentile(&one, 99) - 7.0).abs() < f64::EPSILON);
        let two = [1.0, 2.0];
        assert!(
            (percentile(&two, 50) - 1.0).abs() < f64::EPSILON,
            "ceil(0.5·2)=1 → first"
        );
        assert!(
            (percentile(&two, 95) - 2.0).abs() < f64::EPSILON,
            "ceil(0.95·2)=2 → second"
        );
    }
}
