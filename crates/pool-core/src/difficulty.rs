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
        }
    }

    /// Record a share submission. Returns Some(new_difficulty) if a retarget is needed.
    pub fn record_share(&mut self) -> Option<f64> {
        self.shares_in_window += 1;

        let elapsed = self.window_start.elapsed().as_secs_f64();

        // Early retarget: if we've already received 4x the expected shares
        // before the retarget interval, retarget immediately to avoid flooding.
        let expected_in_elapsed =
            self.target_shares_per_minute * elapsed / 60.0;
        let early_trigger = self.shares_in_window as f64 > expected_in_elapsed * 4.0
            && elapsed >= 5.0;

        if !early_trigger && elapsed < self.retarget_interval_secs {
            return None;
        }

        let shares_per_minute = (self.shares_in_window as f64 / elapsed) * 60.0;
        let ratio = shares_per_minute / self.target_shares_per_minute;

        let new_difficulty = if ratio > 4.0 || ratio < 0.25 {
            // RAMP-UP: way off target, aggressive jump to converge fast.
            let adjustment = ratio.clamp(0.25, 16.0);
            self.smoothed_ratio = 1.0; // reset EMA after big jump
            (self.current_difficulty * adjustment)
                .clamp(self.min_difficulty, self.max_difficulty)
        } else {
            // STEADY-STATE: use EMA to smooth out variance.
            // Alpha = 0.3 means ~30% weight on new sample, 70% on history.
            let alpha = 0.3;
            self.smoothed_ratio = alpha * ratio + (1.0 - alpha) * self.smoothed_ratio;

            // Dead zone: if smoothed ratio is between 0.8 and 1.2, don't change.
            if self.smoothed_ratio > 0.8 && self.smoothed_ratio < 1.2 {
                self.shares_in_window = 0;
                self.window_start = Instant::now();
                return None;
            }

            // Gentle clamp: max 1.5x up, 0.67x down per interval.
            let adjustment = self.smoothed_ratio.clamp(0.67, 1.5);
            (self.current_difficulty * adjustment)
                .clamp(self.min_difficulty, self.max_difficulty)
        };

        self.current_difficulty = new_difficulty;
        self.shares_in_window = 0;
        self.window_start = Instant::now();

        Some(new_difficulty)
    }

    pub fn current_difficulty(&self) -> f64 {
        self.current_difficulty
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
