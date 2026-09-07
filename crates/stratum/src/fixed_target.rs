//! Immutable assigned-share target for fixed-target PPS sessions and jobs.
//! The exact big-endian bytes are authoritative. Floating difficulty is only a
//! legacy Stratum presentation value and must never be converted back into a
//! target or used in financial pricing.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedShareTarget {
    target_be: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FixedTargetError {
    #[error("fixed PPS share target must be nonzero")]
    ZeroTarget,
    #[error("fixed PPS share target cannot change after configuration")]
    AlreadyConfigured,
}

impl FixedShareTarget {
    pub fn new(target_be: [u8; 32]) -> Result<Self, FixedTargetError> {
        if target_be == [0; 32] {
            return Err(FixedTargetError::ZeroTarget);
        }
        Ok(Self { target_be })
    }

    pub fn target_be(self) -> [u8; 32] {
        self.target_be
    }

    pub fn target_hex(self) -> String {
        hex::encode(self.target_be)
    }

    /// Compatibility announcement only, using the pool's established Stratum
    /// difficulty-one convention. It has no validation or monetary authority.
    pub fn display_difficulty(self) -> f64 {
        let target = self
            .target_be
            .iter()
            .fold(0.0_f64, |n, b| n * 256.0 + f64::from(*b));
        (2.0_f64.powi(251) - 1.0) / target
    }

    /// Same inclusive endian convention as pool-core's existing PoW gate.
    /// Caller must independently validate Equihash and immutable job identity.
    pub fn accepts_hash_le(self, hash_le: &[u8; 32]) -> bool {
        let mut hash_be = *hash_le;
        hash_be.reverse();
        hash_be <= self.target_be
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_rejected_and_full_width_target_preserved() {
        assert_eq!(
            FixedShareTarget::new([0; 32]),
            Err(FixedTargetError::ZeroTarget)
        );
        let target = FixedShareTarget::new([255; 32]).unwrap();
        assert_eq!(target.target_be(), [255; 32]);
        assert_eq!(target.target_hex(), "ff".repeat(32));
        assert!(target.display_difficulty().is_finite());
        assert!(target.display_difficulty() > 0.0);
    }

    #[test]
    fn boundary_is_inclusive_and_hash_is_little_endian() {
        let mut target_be = [0; 32];
        target_be[31] = 42;
        let target = FixedShareTarget::new(target_be).unwrap();
        let mut hash_le = [0; 32];
        hash_le[0] = 42;
        assert!(target.accepts_hash_le(&hash_le));
        hash_le[0] = 43;
        assert!(!target.accepts_hash_le(&hash_le));
        hash_le[0] = 41;
        assert!(target.accepts_hash_le(&hash_le));
        assert!(!target.accepts_hash_le(&target_be));
    }

    #[test]
    fn immutable_target_not_reconstructed_from_display_difficulty() {
        let mut bytes = [0; 32];
        bytes[0] = 1;
        bytes[31] = 1;
        let a = FixedShareTarget::new(bytes).unwrap();
        bytes[31] = 2;
        let b = FixedShareTarget::new(bytes).unwrap();
        assert_eq!(a.display_difficulty(), b.display_difficulty());
        assert_ne!(a.target_be(), b.target_be());
        assert_ne!(a.target_hex(), b.target_hex());
    }
}
