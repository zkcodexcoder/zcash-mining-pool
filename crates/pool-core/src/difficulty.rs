use std::time::Instant;

/// Variable difficulty (vardiff) tracker for a single miner.
/// Adjusts the share target to maintain a desired share submission rate.
///
/// Two phases:
/// - **Ramp-up** (ratio > 4x or < 0.25x): aggressive jumps to find the
///   right ballpark quickly for new or mismatched miners.
/// - **Steady-state**: gentle EMA-dampened adjustments with a dead zone
///   (0.8x–1.2x) to avoid oscillation.
pub struct VardiffTracker {
    shares_in_window: u32,
    window_start: Instant,
    target_shares_per_minute: f64,
    retarget_interval_secs: f64,
    current_difficulty: f64,
    min_difficulty: f64,
    max_difficulty: f64,
    /// Smoothed ratio (EMA) for steady-state dampening.
    smoothed_ratio: f64,
    /// Track last retarget direction to detect oscillation.
    /// true = last retarget went up, false = went down, None = no retarget yet.
    last_went_up: Option<bool>,
}

impl VardiffTracker {
    pub fn new(
        target_shares_per_minute: f64,
        retarget_interval_secs: f64,
        initial_difficulty: f64,
    ) -> Self {
        Self {
            shares_in_window: 0,
            window_start: Instant::now(),
            target_shares_per_minute,
            retarget_interval_secs,
            current_difficulty: initial_difficulty,
            min_difficulty: initial_difficulty.min(1.0),
            // 10 billion — enough for miners up to ~1 TH/s
            max_difficulty: 10_000_000_000.0,
            smoothed_ratio: 1.0,
            last_went_up: None,
        }
    }

    /// Record a share submission. Returns Some((new_difficulty, reason)) if a retarget is needed.
    pub fn record_share(&mut self) -> Option<(f64, String)> {
        self.shares_in_window += 1;

        let elapsed = self.window_start.elapsed().as_secs_f64();

        // Early retarget: if we've already received 4x the expected shares
        // before the retarget interval, retarget immediately to avoid flooding.
        //
        // Require at least 3s of elapsed time so the shares/min calculation
        // has a stable sample. With a 1s minimum, a single share submitted
        // quickly after a prior retarget produced wildly inflated spm
        // readings (e.g., 600+ spm from 2 shares in 200ms) and triggered
        // ramp-path jumps that then immediately overcorrected downward.
        let expected_in_elapsed =
            self.target_shares_per_minute * elapsed / 60.0;
        let early_trigger = self.shares_in_window as f64 > expected_in_elapsed * 4.0
            && elapsed >= 3.0;

        if !early_trigger && elapsed < self.retarget_interval_secs {
            return None;
        }

        self.compute_retarget(if early_trigger { "early-trigger" } else { "interval" })
    }

    /// Force an immediate retarget, bypassing the interval and early-trigger gates.
    /// Called when the rate limiter detects a miner submitting too fast.
    pub fn force_retarget(&mut self) -> Option<(f64, String)> {
        self.shares_in_window += 1;
        self.compute_retarget("rate-limit")
    }

    /// Shared retarget calculation used by both `record_share` and `force_retarget`.
    fn compute_retarget(&mut self, trigger: &str) -> Option<(f64, String)> {
        let elapsed = self.window_start.elapsed().as_secs_f64();
        let shares_per_minute = (self.shares_in_window as f64 / elapsed.max(0.01)) * 60.0;

        // Stable zone: if miner is producing 20-100 shares/min AND difficulty
        // is above 10, don't adjust. Only applies once difficulty has ramped up
        // enough — low-difficulty miners need to keep adjusting through this range.
        if shares_per_minute >= 20.0 && shares_per_minute <= 100.0 && self.current_difficulty > 10.0 {
            self.shares_in_window = 0;
            self.window_start = Instant::now();
            return None;
        }

        let ratio = shares_per_minute / self.target_shares_per_minute;

        // Clamp the ratio to max 2x up / 0.5x down per retarget to prevent
        // overshoot oscillation. Converges in a few steps instead of jumping
        // the full ratio and swinging back.
        let clamped_ratio = ratio.clamp(0.5, 2.0);

        let (new_difficulty, phase) = if ratio > 4.0 || ratio < 0.25 {
            // RAMP-UP: far off target, use clamped ratio to step toward
            // the right difficulty without overshooting.
            self.smoothed_ratio = 1.0; // reset EMA after big jump
            let d = (self.current_difficulty * clamped_ratio)
                .clamp(self.min_difficulty, self.max_difficulty);
            (d, "ramp")
        } else {
            // STEADY-STATE: use EMA to smooth out variance.
            // Alpha = 0.15 means ~15% weight on new sample, 85% on history.
            // Low alpha prevents high-hashrate miners from oscillating due to
            // natural variance in share timing.
            let alpha = 0.15;
            self.smoothed_ratio = alpha * ratio + (1.0 - alpha) * self.smoothed_ratio;

            // Dead zone: if smoothed ratio is between 0.6 and 1.5, don't change.
            // Wide zone lets difficulty settle rather than chasing noise.
            if self.smoothed_ratio > 0.6 && self.smoothed_ratio < 1.5 {
                self.shares_in_window = 0;
                self.window_start = Instant::now();
                return None;
            }

            // Gentle clamp: max 1.25x up, 0.8x down per interval.
            let adjustment = self.smoothed_ratio.clamp(0.8, 1.25);
            let d = (self.current_difficulty * adjustment)
                .clamp(self.min_difficulty, self.max_difficulty);
            (d, "ema")
        };

        let going_up = new_difficulty > self.current_difficulty;

        // Oscillation detection: if we're reversing direction, take the
        // midpoint instead of overshooting past the sweet spot again.
        let (final_difficulty, phase) = if let Some(last_up) = self.last_went_up {
            if last_up != going_up {
                let mid = (self.current_difficulty + new_difficulty) / 2.0;
                (mid, format!("{phase}→mid"))
            } else {
                (new_difficulty, phase.to_string())
            }
        } else {
            (new_difficulty, phase.to_string())
        };

        let direction = if final_difficulty > self.current_difficulty { "up" } else { "down" };
        let reason = format!("{} {} ({:.1} spm, ratio {:.2})", trigger, phase, shares_per_minute, ratio);

        self.last_went_up = Some(final_difficulty > self.current_difficulty);
        self.current_difficulty = final_difficulty;
        self.shares_in_window = 0;
        self.window_start = Instant::now();

        Some((final_difficulty, format!("{direction}: {reason}")))
    }

    pub fn current_difficulty(&self) -> f64 {
        self.current_difficulty
    }

    pub fn shares_in_window(&self) -> u32 {
        self.shares_in_window
    }

    pub fn smoothed_ratio(&self) -> f64 {
        self.smoothed_ratio
    }

    pub fn window_elapsed_secs(&self) -> f64 {
        self.window_start.elapsed().as_secs_f64()
    }
}

/// Convert a difficulty value to a 256-bit target (big-endian hex string).
/// target = powLimit / difficulty
pub fn difficulty_to_target_hex(difficulty: f64) -> String {
    if difficulty <= 0.0 {
        return "ff".repeat(32);
    }

    // Zcash powLimit = 2^251 - 1, represented as a big-endian 32-byte value.
    // For simplicity, we use an approximation.
    // powLimit hex: 0007ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff

    let pow_limit_f64: f64 = 2.0f64.powi(251) - 1.0;
    let target_val = pow_limit_f64 / difficulty;

    // Convert f64 to 32-byte big-endian representation
    let mut target = [0u8; 32];
    let mut remaining = target_val;
    for i in 0..32 {
        let divisor = 256.0f64.powi((31 - i) as i32);
        if divisor > 0.0 {
            let b = (remaining / divisor).min(255.0) as u8;
            target[i] = b;
            remaining -= (b as f64) * divisor;
        }
    }

    hex::encode(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vardiff_no_retarget_before_interval() {
        let mut v = VardiffTracker::new(10.0, 30.0, 1.0);
        assert!(v.record_share().is_none());
        assert_eq!(v.current_difficulty(), 1.0);
    }

    #[test]
    fn difficulty_to_target_valid() {
        let target = difficulty_to_target_hex(1.0);
        assert_eq!(target.len(), 64);
    }
}
