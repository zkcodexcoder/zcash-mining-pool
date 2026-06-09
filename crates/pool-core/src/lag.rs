//! Template-to-notify lag tracking.
//!
//! Measures the time between zebrad's `getblocktemplate` longpoll returning
//! with a new tip and the pool actually calling `stratum.broadcast_notify`.
//! That window is the orphan-risk surface area we control: any miner share
//! found against the *old* template after a block has landed elsewhere will
//! be orphaned, so we want it as small as humanly possible and we want
//! visibility into it.
//!
//! Snapshots are persisted to the `pool_status` SQLite table as JSON so the
//! dashboard process (separate binary) can read them — there's no shared
//! in-process state between `zcash-pool` and `zcash-dashboard`.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Cap on retained samples. ~200 covers ~4 hours of blocks (75 s spacing)
/// which is plenty for stable p95; older samples drop off the front.
const MAX_SAMPLES: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LagKind {
    /// Race-to-tip empty-block notify — the one that switches miners onto
    /// the new parent. This is the orphan-window-relevant measurement.
    EmptyBlock,
    /// Full-template notify with mempool txs — fires immediately after the
    /// empty-block one with `clean_jobs=false`. Useful for spotting tag
    /// injection / merkle root computation regressions.
    FullTemplate,
}

#[derive(Debug, Clone, Copy)]
struct Sample {
    recorded_at: Instant,
    lag: Duration,
    height: u64,
    kind: LagKind,
}

#[derive(Debug, Default)]
pub struct TemplateLagTracker {
    samples: Mutex<VecDeque<Sample>>,
    samples_total: AtomicU64,
}

/// Snapshot of recent lag stats — what gets serialised into `pool_status`
/// and read by the dashboard.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemplateLagSnapshot {
    /// Total samples observed since pool start (uncapped).
    pub samples_total: u64,
    /// Sample count currently held in the sliding window (≤ MAX_SAMPLES).
    pub samples_window: usize,
    /// Seconds since the most recent sample landed. None if no samples yet.
    pub last_sample_age_secs: Option<u64>,
    /// Block height of the most recent sample.
    pub last_height: Option<u64>,
    pub last_empty_ms: Option<f64>,
    pub p50_empty_ms: Option<f64>,
    pub p95_empty_ms: Option<f64>,
    pub last_full_ms: Option<f64>,
    pub p50_full_ms: Option<f64>,
    pub p95_full_ms: Option<f64>,
}

impl TemplateLagTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&self, lag: Duration, height: u64, kind: LagKind) {
        let mut samples = self.samples.lock().expect("lag samples mutex poisoned");
        samples.push_back(Sample {
            recorded_at: Instant::now(),
            lag,
            height,
            kind,
        });
        while samples.len() > MAX_SAMPLES {
            samples.pop_front();
        }
        self.samples_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> TemplateLagSnapshot {
        let mut empty_lags: Vec<f64> = Vec::with_capacity(MAX_SAMPLES);
        let mut full_lags: Vec<f64> = Vec::with_capacity(MAX_SAMPLES);
        let mut last_empty_ms = None;
        let mut last_full_ms = None;
        let last_sample_age_secs;
        let last_height;
        let samples_window;

        {
            let samples = self.samples.lock().expect("lag samples mutex poisoned");
            samples_window = samples.len();
            let now = Instant::now();
            last_sample_age_secs = samples
                .back()
                .map(|s| now.saturating_duration_since(s.recorded_at).as_secs());
            last_height = samples.back().map(|s| s.height);
            for s in samples.iter() {
                let ms = s.lag.as_secs_f64() * 1000.0;
                match s.kind {
                    LagKind::EmptyBlock => {
                        empty_lags.push(ms);
                        last_empty_ms = Some(ms);
                    }
                    LagKind::FullTemplate => {
                        full_lags.push(ms);
                        last_full_ms = Some(ms);
                    }
                }
            }
        }

        TemplateLagSnapshot {
            samples_total: self.samples_total.load(Ordering::Relaxed),
            samples_window,
            last_sample_age_secs,
            last_height,
            last_empty_ms,
            p50_empty_ms: percentile(&mut empty_lags, 0.50),
            p95_empty_ms: percentile(&mut empty_lags, 0.95),
            last_full_ms,
            p50_full_ms: percentile(&mut full_lags, 0.50),
            p95_full_ms: percentile(&mut full_lags, 0.95),
        }
    }
}

/// Nearest-rank percentile on f64s, clamped to `[0.0, 1.0]`. Sorts the slice
/// in place — caller's `Vec` is consumed for this snapshot.
fn percentile(values: &mut [f64], pct: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pct = pct.clamp(0.0, 1.0);
    let idx = ((values.len() as f64 - 1.0) * pct).round() as usize;
    Some(values[idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tracker_returns_empty_snapshot() {
        let t = TemplateLagTracker::new();
        let snap = t.snapshot();
        assert_eq!(snap.samples_total, 0);
        assert_eq!(snap.samples_window, 0);
        assert!(snap.last_empty_ms.is_none());
        assert!(snap.p50_empty_ms.is_none());
        assert!(snap.p95_full_ms.is_none());
    }

    #[test]
    fn tracker_records_and_computes_quantiles() {
        let t = TemplateLagTracker::new();
        // Ten empty-block samples 10..=100 ms.
        for i in 1..=10 {
            t.record(
                Duration::from_millis(i * 10),
                1_000 + i,
                LagKind::EmptyBlock,
            );
        }
        let snap = t.snapshot();
        assert_eq!(snap.samples_total, 10);
        assert_eq!(snap.samples_window, 10);
        assert_eq!(snap.last_empty_ms, Some(100.0));
        // Nearest-rank p50 on len=10 → idx round(9*0.5) = 5 → 6th value = 60ms.
        assert_eq!(snap.p50_empty_ms, Some(60.0));
        // p95 → idx round(9*0.95) = 9 → last value = 100ms.
        assert_eq!(snap.p95_empty_ms, Some(100.0));
        // Full-template stream empty.
        assert!(snap.last_full_ms.is_none());
        assert!(snap.p50_full_ms.is_none());
    }

    #[test]
    fn tracker_caps_window_but_keeps_cumulative_count() {
        let t = TemplateLagTracker::new();
        let total = MAX_SAMPLES + 50;
        for i in 0..total {
            t.record(
                Duration::from_millis(i as u64),
                1_000,
                LagKind::FullTemplate,
            );
        }
        let snap = t.snapshot();
        assert_eq!(snap.samples_window, MAX_SAMPLES);
        assert_eq!(snap.samples_total, total as u64);
        // The oldest 50 samples should have been dropped; window now holds
        // samples 50..250, so min lag = 50 ms.
        let snap2 = t.snapshot();
        let p50 = snap2.p50_full_ms.unwrap();
        assert!(p50 >= 50.0, "p50 {} should be >= 50ms after window slide", p50);
    }

    #[test]
    fn mixed_kinds_are_partitioned_in_snapshot() {
        let t = TemplateLagTracker::new();
        t.record(Duration::from_millis(10), 1, LagKind::EmptyBlock);
        t.record(Duration::from_millis(50), 1, LagKind::FullTemplate);
        t.record(Duration::from_millis(20), 2, LagKind::EmptyBlock);
        t.record(Duration::from_millis(60), 2, LagKind::FullTemplate);
        let snap = t.snapshot();
        assert_eq!(snap.samples_total, 4);
        assert_eq!(snap.last_empty_ms, Some(20.0));
        assert_eq!(snap.last_full_ms, Some(60.0));
        assert_eq!(snap.last_height, Some(2));
    }
}
