//! Adaptive simulation detail (LOD): the per-tick, stateless activity mask.
//!
//! Phase 4 groundwork (docs/research/adaptive-detail.md, settled by
//! Q-2026-07-06-A). Quiet regions of the world tick less often to save work.
//! A "chunk" is a `chunk_cells × chunk_cells` block of grid cells (pure
//! geometry — see `GridGeom::chunk_of`). Each tick every chunk is classified
//! by an activity metric and assigned a tick stride from the policy ladder;
//! a particle is *active this tick* iff its chunk's stride divides the tick.
//!
//! Determinism (the whole reason the mask is stateless):
//! - The chunk metric is `max` over the chunk's particles of a non-negative
//!   per-particle scalar. `max` is associative and commutative and exact in
//!   f32, so the reduction is order-independent — thread count cannot change
//!   it (this pass is sequential today, but the invariant holds regardless).
//! - The mask is a pure function of `(state, policy, tick)`. Nothing about it
//!   is saved: a resumed run recomputes the identical mask from restored state
//!   on its first tick, so save/resume stays bit-identical.
//! - Frozen (inactive) particles do not move, so their cell — and therefore
//!   their chunk — is unchanged by the next canonicalize; the layout stays a
//!   pure function of state.

use genesis_config::LodPolicy;

use crate::grid::GridGeom;
use crate::store::ParticleStore;

/// Per-particle activity scalar: speed². Non-negative and finite (the
/// information clamp keeps velocities NaN-free), so it is a clean `max` input.
#[inline]
fn activity(vx: f32, vy: f32) -> f32 {
    vx * vx + vy * vy
}

/// Fill `store.active` for this tick.
///
/// Must run after `canonicalize` (it reads `store.cell`). With the policy
/// disabled the mask is emptied — downstream passes read an empty mask as
/// "everything active", identical to running without LOD at all. With the
/// policy enabled the mask is materialised to length `n`.
pub fn classify(store: &mut ParticleStore, geom: &GridGeom, policy: &LodPolicy, tick: u64) {
    store.active.clear();
    if !policy.enabled {
        // Empty mask = "not tracked" = all active. Cheaper than filling `true`
        // and lets the disabled path stay perfectly hash-neutral.
        return;
    }

    let n = store.len();
    let chunk_cells = policy.chunk_cells;
    let chunk_count = geom.chunk_count(chunk_cells);

    // Per-chunk activity metric: max speed² over the chunk's particles. Empty
    // chunks keep metric 0 and take the coldest rate.
    let mut metric = vec![0.0f32; chunk_count];
    for i in 0..n {
        let chunk = geom.chunk_of(store.cell[i], chunk_cells) as usize;
        let a = activity(store.vx[i], store.vy[i]);
        if a > metric[chunk] {
            metric[chunk] = a;
        }
    }

    // Per-chunk tick stride from the ladder, then the per-particle mask.
    let rates: Vec<u32> = metric.iter().map(|&m| policy.rate_for(m)).collect();
    store.active.reserve(n);
    for i in 0..n {
        let chunk = geom.chunk_of(store.cell[i], chunk_cells) as usize;
        let rate = rates[chunk] as u64;
        store.active.push(tick.is_multiple_of(rate));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use genesis_config::LodRung;

    fn ladder_policy() -> LodPolicy {
        LodPolicy {
            enabled: true,
            chunk_cells: 1, // one chunk per grid cell — sharpest classification
            ladder: vec![
                LodRung {
                    min_activity: 0.0,
                    rate: 4,
                },
                LodRung {
                    min_activity: 1.0,
                    rate: 1,
                },
            ],
        }
    }

    /// Build a canonicalized 3x3-cell store with the given (x, y, vx, vy)
    /// particles. Cell size 10, world 30x30.
    fn store_with(parts: &[(f32, f32, f32, f32)]) -> (ParticleStore, GridGeom) {
        let geom = GridGeom::new(30.0, 30.0, 10.0);
        let mut s = ParticleStore::default();
        for (k, &(x, y, vx, vy)) in parts.iter().enumerate() {
            s.push(k as u64, x, y, vx, vy, 1.0, 0.0, 0.0);
        }
        s.canonicalize(&geom);
        (s, geom)
    }

    #[test]
    fn disabled_policy_leaves_mask_empty() {
        let (mut s, geom) = store_with(&[(1.0, 1.0, 5.0, 0.0)]);
        let mut p = ladder_policy();
        p.enabled = false;
        classify(&mut s, &geom, &p, 3);
        assert!(s.active.is_empty(), "disabled LOD must not track a mask");
    }

    #[test]
    fn hot_chunk_active_every_tick_cold_chunk_strided() {
        // One fast particle (speed² = 25 ≥ 1 → rate 1) in cell (0,0); one slow
        // particle (speed² = 0 → rate 4) in cell (2,2). chunk_cells = 1, so
        // each sits in its own chunk.
        let (mut s, geom) = store_with(&[(1.0, 1.0, 5.0, 0.0), (25.0, 25.0, 0.0, 0.0)]);
        let hot = s.id.iter().position(|&x| x == 0).unwrap();
        let cold = s.id.iter().position(|&x| x == 1).unwrap();

        // Tick 0: 0 % 4 == 0 and 0 % 1 == 0 → both active.
        classify(&mut s, &geom, &ladder_policy(), 0);
        assert!(s.active[hot]);
        assert!(s.active[cold]);

        // Tick 1: hot active (rate 1), cold frozen (1 % 4 != 0).
        classify(&mut s, &geom, &ladder_policy(), 1);
        assert!(s.active[hot], "hot chunk must run every tick");
        assert!(!s.active[cold], "cold chunk must be frozen off-stride");

        // Tick 4: cold active again (4 % 4 == 0).
        classify(&mut s, &geom, &ladder_policy(), 4);
        assert!(s.active[cold], "cold chunk must re-wake on its stride");
    }

    #[test]
    fn mask_length_matches_particle_count() {
        let (mut s, geom) = store_with(&[
            (1.0, 1.0, 0.0, 0.0),
            (5.0, 5.0, 2.0, 0.0),
            (25.0, 25.0, 0.0, 3.0),
        ]);
        classify(&mut s, &geom, &ladder_policy(), 2);
        assert_eq!(s.active.len(), s.len());
    }

    #[test]
    fn chunk_metric_is_max_over_particles() {
        // Two particles share cell (0,0): one fast, one slow. With chunk_cells
        // = 1 they share a chunk; the chunk's max activity (fast) makes BOTH
        // active every tick, including off the cold stride.
        let (mut s, geom) = store_with(&[(1.0, 1.0, 5.0, 0.0), (2.0, 2.0, 0.0, 0.0)]);
        classify(&mut s, &geom, &ladder_policy(), 1); // 1 % 4 != 0
        assert!(
            s.active.iter().all(|&a| a),
            "one fast particle must keep its whole chunk hot"
        );
    }

    #[test]
    fn classification_is_thread_count_stable() {
        // The mask is computed sequentially, but assert the property directly:
        // same state + tick + policy → identical mask, however it is built.
        let parts = [
            (1.0, 1.0, 5.0, 0.0),
            (12.0, 3.0, 0.0, 0.0),
            (25.0, 25.0, 0.5, 0.5),
        ];
        let (mut a, geom) = store_with(&parts);
        let (mut b, _) = store_with(&parts);
        classify(&mut a, &geom, &ladder_policy(), 7);
        classify(&mut b, &geom, &ladder_policy(), 7);
        assert_eq!(a.active, b.active);
    }
}
