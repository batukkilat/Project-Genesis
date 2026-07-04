//! Torus (wrapping) world math.
//!
//! The world is a 2D torus: both axes wrap, there are no edges. All position
//! arithmetic in the simulation must go through these helpers so wrapping is
//! consistent everywhere.

use crate::Vec2;

/// Wrap a coordinate into `[0, size)`.
///
/// Handles any input magnitude and the float edge case where rounding in
/// `rem_euclid` lands exactly on `size`.
pub fn wrap(x: f32, size: f32) -> f32 {
    let r = x.rem_euclid(size);
    if r >= size { 0.0 } else { r }
}

pub fn wrap_vec(v: Vec2, size: Vec2) -> Vec2 {
    Vec2::new(wrap(v.x, size.x), wrap(v.y, size.y))
}

/// Shortest signed displacement from `a` to `b` on a circle of circumference
/// `size`. Result is in `[-size/2, size/2]`. Both inputs must already be
/// wrapped into `[0, size)`.
///
/// Antisymmetry is exact: `delta(b, a) == -delta(a, b)` bit-for-bit, which is
/// what makes pairwise forces conserve momentum.
pub fn delta(a: f32, b: f32, size: f32) -> f32 {
    let d = b - a;
    let half = 0.5 * size;
    if d > half {
        d - size
    } else if d < -half {
        d + size
    } else {
        d
    }
}

/// Shortest displacement vector from `a` to `b` on the torus.
pub fn delta_vec(a: Vec2, b: Vec2, size: Vec2) -> Vec2 {
    Vec2::new(delta(a.x, b.x, size.x), delta(a.y, b.y, size.y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_bounds() {
        assert_eq!(wrap(0.0, 10.0), 0.0);
        assert_eq!(wrap(10.0, 10.0), 0.0);
        assert_eq!(wrap(12.5, 10.0), 2.5);
        assert_eq!(wrap(-0.5, 10.0), 9.5);
        assert_eq!(wrap(-20.0, 10.0), 0.0);
        // Tiny negative values can round to `size` inside rem_euclid.
        let w = wrap(-1e-7, 4096.0);
        assert!((0.0..4096.0).contains(&w));
    }

    #[test]
    fn delta_shortest_path() {
        assert_eq!(delta(1.0, 4.0, 10.0), 3.0);
        assert_eq!(delta(4.0, 1.0, 10.0), -3.0);
        // Across the seam: 9.5 -> 0.5 is +1, not -9.
        assert_eq!(delta(9.5, 0.5, 10.0), 1.0);
        assert_eq!(delta(0.5, 9.5, 10.0), -1.0);
    }

    #[test]
    fn delta_antisymmetric_exact() {
        let size = 4096.0;
        let cases = [(0.1, 4095.9), (100.0, 2148.0), (2048.0, 0.0), (7.25, 7.5)];
        for (a, b) in cases {
            assert_eq!(delta(a, b, size).to_bits(), (-delta(b, a, size)).to_bits());
        }
    }

    #[test]
    fn delta_within_half() {
        for i in 0..100 {
            let a = i as f32 * 0.37 % 10.0;
            let b = i as f32 * 1.93 % 10.0;
            let d = delta(a, b, 10.0);
            assert!(d.abs() <= 5.0);
        }
    }
}
