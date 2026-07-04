//! Deterministic, splittable RNG.
//!
//! Implementation is SplitMix64 (Steele, Lea & Flood, "Fast Splittable
//! Pseudorandom Number Generators", OOPSLA 2014). Chosen because:
//!
//! - The algorithm is fully specified here, in this file. No external crate
//!   can change the stream under us in a version bump.
//! - `split()` derives statistically independent child streams, which is how
//!   per-system and per-chunk streams stay deterministic under parallelism
//!   (each chunk owns a stream; execution order stops mattering).
//!
//! Everything random in the simulation must flow from one master seed through
//! this type. There is no global RNG.

/// Golden-ratio increment; the canonical SplitMix64 gamma.
const GOLDEN_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// SplitMix64 finalizer (Stafford's Mix13 variant).
fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Mix used to derive gammas for split streams; keeps them odd and well mixed.
fn mix_gamma(z: u64) -> u64 {
    let z = mix64(z) | 1;
    // If the candidate gamma has too few bit transitions, XOR-fold it
    // (as in the reference implementation) to avoid weak increments.
    if (z ^ (z >> 1)).count_ones() < 24 {
        z ^ 0xAAAA_AAAA_AAAA_AAAA
    } else {
        z
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetRng {
    state: u64,
    gamma: u64,
}

impl DetRng {
    /// Root stream from a master seed.
    pub fn new(seed: u64) -> Self {
        DetRng {
            state: mix64(seed),
            gamma: GOLDEN_GAMMA,
        }
    }

    /// Derive an independent child stream. Advances this stream.
    pub fn split(&mut self) -> DetRng {
        let state = self.next_u64();
        let gamma = mix_gamma(self.next_u64());
        DetRng { state, gamma }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(self.gamma);
        mix64(self.state)
    }

    /// Uniform in [0, 1). Uses the top 24 bits, exactly representable in f32.
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 * (1.0 / (1u32 << 24) as f32)
    }

    /// Uniform in [lo, hi). Returns `lo` when the range is empty.
    pub fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next_f32()
    }

    /// Internal state for serialization. Round-trips via [`DetRng::from_parts`].
    pub fn to_parts(&self) -> (u64, u64) {
        (self.state, self.gamma)
    }

    pub fn from_parts(state: u64, gamma: u64) -> Self {
        DetRng { state, gamma }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = DetRng::new(42);
        let mut b = DetRng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = DetRng::new(1);
        let mut b = DetRng::new(2);
        let same = (0..100).filter(|_| a.next_u64() == b.next_u64()).count();
        assert_eq!(same, 0);
    }

    #[test]
    fn split_streams_are_independent_and_deterministic() {
        let mut root1 = DetRng::new(7);
        let mut root2 = DetRng::new(7);
        let mut child1 = root1.split();
        let mut child2 = root2.split();
        for _ in 0..1000 {
            assert_eq!(child1.next_u64(), child2.next_u64());
        }
        // Child and parent must not produce the same stream.
        let mut root = DetRng::new(7);
        let mut child = root.split();
        let overlap = (0..100)
            .filter(|_| root.next_u64() == child.next_u64())
            .count();
        assert_eq!(overlap, 0);
    }

    #[test]
    fn f32_range_bounds() {
        let mut rng = DetRng::new(3);
        for _ in 0..10_000 {
            let v = rng.next_f32();
            assert!((0.0..1.0).contains(&v));
            let r = rng.range_f32(-5.0, 5.0);
            assert!((-5.0..5.0).contains(&r));
        }
    }

    #[test]
    fn parts_roundtrip() {
        let mut a = DetRng::new(9);
        a.next_u64();
        let (s, g) = a.to_parts();
        let mut b = DetRng::from_parts(s, g);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }
}
