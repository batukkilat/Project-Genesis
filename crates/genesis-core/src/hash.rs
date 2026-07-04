//! Canonical state hash (FNV-1a, 64-bit).
//!
//! Used to verify deterministic replay: two runs are considered identical
//! when their per-tick state hashes match. The algorithm is fixed here so the
//! hash of a given state never changes across dependency updates. It is a
//! test/verification tool, not a cryptographic guarantee.

const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;

#[derive(Debug, Clone)]
pub struct StateHasher {
    state: u64,
}

impl StateHasher {
    pub fn new() -> Self {
        StateHasher { state: FNV_OFFSET }
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.state ^= b as u64;
            self.state = self.state.wrapping_mul(FNV_PRIME);
        }
    }

    pub fn write_u32(&mut self, v: u32) {
        self.write_bytes(&v.to_le_bytes());
    }

    pub fn write_u64(&mut self, v: u64) {
        self.write_bytes(&v.to_le_bytes());
    }

    /// Hashes the exact bit pattern, so -0.0 and 0.0 hash differently.
    /// That is intentional: replay identity means bit-identical state.
    pub fn write_f32(&mut self, v: f32) {
        self.write_u32(v.to_bits());
    }

    pub fn finish(&self) -> u64 {
        self.state
    }
}

impl Default for StateHasher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_known_value() {
        // FNV-1a of "a" is a published test vector — pinned so accidental
        // algorithm changes fail loudly.
        let mut h = StateHasher::new();
        h.write_bytes(b"a");
        assert_eq!(h.finish(), 0xAF63_DC4C_8601_EC8C);

        let again = {
            let mut h2 = StateHasher::new();
            h2.write_bytes(b"a");
            h2.finish()
        };
        assert_eq!(h.finish(), again);
    }

    #[test]
    fn order_sensitive() {
        let mut a = StateHasher::new();
        a.write_u32(1);
        a.write_u32(2);
        let mut b = StateHasher::new();
        b.write_u32(2);
        b.write_u32(1);
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn float_bit_patterns_distinct() {
        let mut a = StateHasher::new();
        a.write_f32(0.0);
        let mut b = StateHasher::new();
        b.write_f32(-0.0);
        assert_ne!(a.finish(), b.finish());
    }
}
