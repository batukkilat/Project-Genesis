//! Asteroid impact application (Q-2026-07-09-A; shape settled in the
//! decisions log 2026-07-06).
//!
//! An impact is a player action: a tick-stamped, replay-recorded event
//! delivering a momentum + energy shock to existing particles plus a
//! particle payload whose "material" is quantity ranges — a region of
//! quantity space, never a named substance. Matter and energy arrive from
//! outside the world by design (external material); the injection is exactly
//! the payload quantities plus the shock energy, which tests account for.
//!
//! Determinism:
//! - The shock writes only per-particle slots from values independent of
//!   walk order (each particle's own distance to the impact point). The
//!   one order-sensitive value — the falloff-weight total the energy
//!   deposit normalizes by — is summed in particle-id order, because the
//!   store layout at drain time is NOT a pure function of state: an
//!   uninterrupted run drains on the previous tick's canonical layout,
//!   a resumed run on the snapshot's id-sorted layout. Id order is the
//!   ordering both share, so a save taken on the impact's own tick
//!   resumes bit-for-bit (regression-tested).
//! - The payload draws from a stream derived as
//!   `derive(stream_seed, [IMPACT_STREAM, tick, queue_index])`: order-free,
//!   collision-free with interaction streams (distinct leading tag), and
//!   identical whether the action arrives from a script, a resumed save, or
//!   the future UI.
//! - Applied in the start-of-tick drain, before canonicalize — the pushed
//!   payload marks the store dirty, so the layout is rebuilt by full sort.

use genesis_config::PayloadSpec;
use genesis_core::{DetRng, torus};

use crate::grid::GridGeom;
use crate::store::ParticleStore;

/// Leading derivation salt for impact payload streams. Interaction streams
/// derive from 4-element `[tick, id_i, id_j, rule]` salts whose first element
/// is a tick; impacts use a fixed tag far above any plausible tick to keep
/// the stream families disjoint.
const IMPACT_STREAM: u64 = u64::MAX - 0x494D5041; // "IMPA"

/// Leading derivation salt for rift payload streams (Q-2026-07-10-C) —
/// distinct from impacts so an impact and a rift on the same tick and queue
/// index draw independent streams.
const RIFT_STREAM: u64 = u64::MAX - 0x52494654; // "RIFT"

/// Apply one impact to the world. `queue_index` is the action's position in
/// this tick's drain (script order), part of the payload stream derivation so
/// two impacts on the same tick draw independent streams.
#[allow(clippy::too_many_arguments)]
pub fn apply(
    store: &mut ParticleStore,
    geom: &GridGeom,
    x: f32,
    y: f32,
    radius: f32,
    impulse: f32,
    energy: f32,
    payload: &PayloadSpec,
    stream_seed: u64,
    tick: u64,
    queue_index: u64,
    next_id: &mut u64,
) {
    let x = torus::wrap(x, geom.world_w);
    let y = torus::wrap(y, geom.world_h);

    // --- Shock: radial momentum impulse + energy deposit, linear falloff.
    // Gather hits first: the deposit normalizes by the falloff-weight
    // total, an f32 sum whose bits depend on summation order — and store
    // index order is NOT a pure function of state at drain time (an
    // uninterrupted run drains on the previous tick's canonical layout, a
    // resumed run on the snapshot's id-sorted layout). Summing in id order
    // makes the total identical across both, so a save taken on the impact
    // tick resumes bit-for-bit.
    let n = store.len();
    let mut hits: Vec<(u64, usize, f32, f32, f32)> = Vec::new();
    for i in 0..n {
        if let Some((w, dx, dy)) = shock_weight(store, geom, i, x, y, radius) {
            hits.push((store.id[i], i, w, dx, dy));
        }
    }
    hits.sort_unstable_by_key(|&(id, ..)| id);
    let weight_total: f32 = hits.iter().map(|&(_, _, w, ..)| w).sum();
    // Membership is positive falloff weight (shock_weight), so any hit
    // implies a positive total: the declared energy deposits in full
    // whenever someone is struck, and is lost entirely otherwise.
    if weight_total > 0.0 {
        for &(_, i, w, dx, dy) in &hits {
            // Radially outward unit direction; a particle exactly at the
            // point has no direction and takes no impulse (weight 1, so
            // it still receives its energy share).
            let d = (dx * dx + dy * dy).sqrt();
            if d > 0.0 {
                let dv = impulse * w / store.matter[i];
                store.vx[i] += dv * dx / d;
                store.vy[i] += dv * dy / d;
            }
            store.energy[i] += energy * w / weight_total;
        }
    }

    // --- Payload: `count` particles on the spread disc, ejected radially.
    let mut rng = DetRng::derive(stream_seed, &[IMPACT_STREAM, tick, queue_index]);
    for _ in 0..payload.count {
        // Uniform over the disc (sqrt-area), uniform angle.
        let angle = rng.range_f32(0.0, std::f32::consts::TAU);
        let dist = payload.spread * rng.range_f32(0.0, 1.0).sqrt();
        let speed = rng.range_f32(payload.speed.lo, payload.speed.hi);
        let matter = rng.range_f32(payload.matter.lo, payload.matter.hi);
        let e = rng.range_f32(payload.energy.lo, payload.energy.hi);
        let info = rng.range_f32(payload.information.lo, payload.information.hi);
        let (dir_x, dir_y) = (angle.cos(), angle.sin());
        let id = *next_id;
        *next_id += 1;
        store.push(
            id,
            torus::wrap(x + dir_x * dist, geom.world_w),
            torus::wrap(y + dir_y * dist, geom.world_h),
            dir_x * speed,
            dir_y * speed,
            matter,
            e,
            info,
        );
    }
}

/// Apply one tectonic rift (Q-2026-07-10-C): an impact-shaped shock whose
/// source is a segment. Every determinism rule of [`apply`] holds
/// unchanged — id-order weight sum, order-free payload stream (distinct
/// tag), drain-time application. The segment vector is taken exactly as
/// authored ((x1 - x0, y1 - y0), never wrapped); each particle's offset to
/// the segment's start uses the torus metric, so a segment authored across
/// the seam shocks across the seam. A degenerate segment is a point impact.
#[allow(clippy::too_many_arguments)]
pub fn apply_rift(
    store: &mut ParticleStore,
    geom: &GridGeom,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    radius: f32,
    impulse: f32,
    energy: f32,
    payload: &PayloadSpec,
    stream_seed: u64,
    tick: u64,
    queue_index: u64,
    next_id: &mut u64,
) {
    let ax = torus::wrap(x0, geom.world_w);
    let ay = torus::wrap(y0, geom.world_h);
    // Authored segment vector, taken literally.
    let (sx, sy) = (x1 - x0, y1 - y0);
    let seg_len2 = sx * sx + sy * sy;

    // --- Shock: perpendicular momentum impulse + energy deposit, linear
    // falloff on distance to the segment. Same id-order weight sum as the
    // point impact, for the same save-on-impact-tick reason.
    let n = store.len();
    let mut hits: Vec<(u64, usize, f32, f32, f32)> = Vec::new();
    for i in 0..n {
        // Offset from the segment start to the particle; the closest
        // segment point comes from projecting onto the literal segment
        // vector. Beyond the ends this degrades to point distance from
        // the nearest endpoint, exactly like an impact there.
        //
        // `torus::delta` folds the offset to the torus-shortest wrap, but
        // the projection needs the offset relative to the *unfolded*
        // segment: for a particle whose along-segment offset from the
        // start exceeds half the world (possible once segment length +
        // radius does), the folded offset flips sign, `t` clamps to the
        // wrong end, and the far end-cap is silently missed (the shipped
        // rift.ron authors exactly a half-world segment). The true torus
        // distance to the segment is the minimum over the 3×3 wrap copies
        // of the offset — sufficient for any segment up to a full world
        // period per axis (validated at assembly). The folded (0,0) copy
        // is evaluated first with the original expression and ties keep
        // it, so every safe segment reproduces its old bits exactly.
        let rx = torus::delta(ax, store.px[i], geom.world_w);
        let ry = torus::delta(ay, store.py[i], geom.world_h);
        let project = |cx: f32, cy: f32| {
            let t = if seg_len2 > 0.0 {
                ((cx * sx + cy * sy) / seg_len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let dx = cx - t * sx;
            let dy = cy - t * sy;
            (dx * dx + dy * dy, dx, dy)
        };
        let (mut d2, mut dx, mut dy) = project(rx, ry);
        for ky in [-1.0f32, 0.0, 1.0] {
            for kx in [-1.0f32, 0.0, 1.0] {
                if kx == 0.0 && ky == 0.0 {
                    continue;
                }
                let (c_d2, c_dx, c_dy) = project(rx + kx * geom.world_w, ry + ky * geom.world_h);
                if c_d2 < d2 {
                    (d2, dx, dy) = (c_d2, c_dx, c_dy);
                }
            }
        }
        if d2 >= radius * radius {
            continue;
        }
        let w = 1.0 - d2.sqrt() / radius;
        if w <= 0.0 {
            continue;
        }
        hits.push((store.id[i], i, w, dx, dy));
    }
    hits.sort_unstable_by_key(|&(id, ..)| id);
    let weight_total: f32 = hits.iter().map(|&(_, _, w, ..)| w).sum();
    if weight_total > 0.0 {
        for &(_, i, w, dx, dy) in &hits {
            // Perpendicularly away from the segment; a particle exactly on
            // it has no direction and takes no impulse (it still receives
            // its energy share) — the point-impact rule, generalized.
            let d = (dx * dx + dy * dy).sqrt();
            if d > 0.0 {
                let dv = impulse * w / store.matter[i];
                store.vx[i] += dv * dx / d;
                store.vy[i] += dv * dy / d;
            }
            store.energy[i] += energy * w / weight_total;
        }
    }

    // --- Payload: each particle is a point-impact payload draw at a
    // uniformly drawn point of the segment (upwelling along the rift).
    let mut rng = DetRng::derive(stream_seed, &[RIFT_STREAM, tick, queue_index]);
    for _ in 0..payload.count {
        let t = rng.range_f32(0.0, 1.0);
        let (cx, cy) = (
            torus::wrap(ax + t * sx, geom.world_w),
            torus::wrap(ay + t * sy, geom.world_h),
        );
        let angle = rng.range_f32(0.0, std::f32::consts::TAU);
        let dist = payload.spread * rng.range_f32(0.0, 1.0).sqrt();
        let speed = rng.range_f32(payload.speed.lo, payload.speed.hi);
        let matter = rng.range_f32(payload.matter.lo, payload.matter.hi);
        let e = rng.range_f32(payload.energy.lo, payload.energy.hi);
        let info = rng.range_f32(payload.information.lo, payload.information.hi);
        let (dir_x, dir_y) = (angle.cos(), angle.sin());
        let id = *next_id;
        *next_id += 1;
        store.push(
            id,
            torus::wrap(cx + dir_x * dist, geom.world_w),
            torus::wrap(cy + dir_y * dist, geom.world_h),
            dir_x * speed,
            dir_y * speed,
            matter,
            e,
            info,
        );
    }
}

/// Falloff weight and torus delta of particle `i` relative to the impact
/// point, or `None` when not struck. Weight is `1 - d/radius`, and being
/// struck is defined as having positive weight — near the rim, f32
/// rounding can put `d/radius` at exactly 1.0 for `d² < radius²`, and a
/// zero-weight "hit" would let a lone rim particle zero the weight total
/// and silently drop the whole energy deposit.
#[inline]
fn shock_weight(
    store: &ParticleStore,
    geom: &GridGeom,
    i: usize,
    x: f32,
    y: f32,
    radius: f32,
) -> Option<(f32, f32, f32)> {
    // Shortest displacement from the impact point to the particle — the
    // outward shock direction (`torus::delta(a, b) = b - a`, wrapped).
    let dx = torus::delta(x, store.px[i], geom.world_w);
    let dy = torus::delta(y, store.py[i], geom.world_h);
    let d2 = dx * dx + dy * dy;
    if d2 >= radius * radius {
        return None;
    }
    let w = 1.0 - d2.sqrt() / radius;
    if w <= 0.0 {
        return None;
    }
    Some((w, dx, dy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use genesis_config::Range;

    fn payload(count: u32) -> PayloadSpec {
        PayloadSpec {
            count,
            matter: Range::new(0.2, 0.8),
            energy: Range::new(1.0, 3.0),
            information: Range::new(0.0, 0.5),
            speed: Range::new(0.5, 2.0),
            spread: 3.0,
        }
    }

    fn world() -> (ParticleStore, GridGeom) {
        let geom = GridGeom::new(100.0, 100.0, 10.0);
        let mut s = ParticleStore::default();
        s.push(0, 50.0, 50.0, 0.0, 0.0, 2.0, 1.0, 0.0); // at impact point
        s.push(1, 55.0, 50.0, 0.0, 0.0, 1.0, 1.0, 0.0); // east, d=5
        s.push(2, 50.0, 42.0, 0.0, 0.0, 0.5, 1.0, 0.0); // north, d=8
        s.push(3, 80.0, 80.0, 0.0, 0.0, 1.0, 1.0, 0.0); // outside
        s.canonicalize(&geom);
        (s, geom)
    }

    #[test]
    fn shock_pushes_radially_with_falloff_and_mass() {
        let (mut s, geom) = world();
        let mut next_id = 4;
        apply(
            &mut s,
            &geom,
            50.0,
            50.0,
            10.0,
            6.0,
            0.0,
            &payload(0),
            7,
            3,
            0,
            &mut next_id,
        );
        let at = |id: u64| s.id.iter().position(|&x| x == id).unwrap();
        // Particle 1: d=5, w=0.5, dv = 6*0.5/1.0 = 3 eastward.
        let i1 = at(1);
        assert!((s.vx[i1] - 3.0).abs() < 1e-5, "vx = {}", s.vx[i1]);
        assert_eq!(s.vy[i1], 0.0);
        // Particle 2: d=8, w=0.2, mass 0.5 → dv = 6*0.2/0.5 = 2.4 northward
        // (negative y — it sits at smaller y than the point).
        let i2 = at(2);
        assert_eq!(s.vx[i2], 0.0);
        assert!((s.vy[i2] + 2.4).abs() < 1e-5, "vy = {}", s.vy[i2]);
        // Particle 0 sits exactly at the point: no direction, no impulse.
        let i0 = at(0);
        assert_eq!((s.vx[i0], s.vy[i0]), (0.0, 0.0));
        // Particle 3 is outside the radius: untouched.
        let i3 = at(3);
        assert_eq!((s.vx[i3], s.vy[i3]), (0.0, 0.0));
        assert_eq!(next_id, 4, "no payload requested");
    }

    #[test]
    fn energy_deposit_totals_exactly_and_weights_by_falloff() {
        let (mut s, geom) = world();
        let before: f32 = s.energy.iter().sum();
        let mut next_id = 4;
        apply(
            &mut s,
            &geom,
            50.0,
            50.0,
            10.0,
            0.0,
            12.0,
            &payload(0),
            7,
            3,
            0,
            &mut next_id,
        );
        let after: f32 = s.energy.iter().sum();
        assert!(
            (after - before - 12.0).abs() < 1e-4,
            "deposited {}",
            after - before
        );
        // Weights: p0 = 1.0, p1 = 0.5, p2 = 0.2 → p0 gets 1.0/1.7 of it.
        let at = |id: u64| s.id.iter().position(|&x| x == id).unwrap();
        let share0 = s.energy[at(0)] - 1.0;
        assert!((share0 - 12.0 / 1.7).abs() < 1e-4, "share0 = {share0}");
        let i3 = at(3);
        assert_eq!(s.energy[i3], 1.0, "outside radius must be untouched");
    }

    #[test]
    fn no_particles_in_radius_deposits_nothing() {
        let (mut s, geom) = world();
        let before: f32 = s.energy.iter().sum();
        let mut next_id = 4;
        apply(
            &mut s,
            &geom,
            10.0,
            10.0,
            2.0,
            5.0,
            99.0,
            &payload(0),
            7,
            3,
            0,
            &mut next_id,
        );
        let after: f32 = s.energy.iter().sum();
        assert_eq!(before, after, "energy must vanish with no target");
    }

    #[test]
    fn payload_spawns_within_ranges_and_disc() {
        let (mut s, geom) = world();
        let mut next_id = 4;
        apply(
            &mut s,
            &geom,
            20.0,
            30.0,
            5.0,
            0.0,
            0.0,
            &payload(64),
            7,
            3,
            0,
            &mut next_id,
        );
        assert_eq!(s.len(), 4 + 64);
        assert_eq!(next_id, 4 + 64, "ids allocated sequentially");
        let p = payload(64);
        for i in 4..s.len() {
            // Outward offset from the impact point to the spawn position.
            let dx = torus::delta(20.0, s.px[i], 100.0);
            let dy = torus::delta(30.0, s.py[i], 100.0);
            let d = (dx * dx + dy * dy).sqrt();
            assert!(d <= p.spread + 1e-4, "particle {i} outside spread: {d}");
            assert!(s.matter[i] >= p.matter.lo && s.matter[i] < p.matter.hi);
            assert!(s.energy[i] >= p.energy.lo && s.energy[i] < p.energy.hi);
            let speed = (s.vx[i] * s.vx[i] + s.vy[i] * s.vy[i]).sqrt();
            assert!(
                speed >= p.speed.lo - 1e-4 && speed < p.speed.hi + 1e-4,
                "speed {speed}"
            );
            // Ejection is radially outward: velocity parallel to offset (or
            // any direction for a center spawn).
            if d > 1e-3 {
                let dot = (dx * s.vx[i] + dy * s.vy[i]) / (d * speed.max(1e-9));
                assert!(dot > 0.999, "not radial: cos = {dot}");
            }
        }
    }

    #[test]
    fn payload_stream_is_deterministic_and_index_distinct() {
        let make = |queue_index: u64| {
            let (mut s, geom) = world();
            let mut next_id = 4;
            apply(
                &mut s,
                &geom,
                20.0,
                30.0,
                5.0,
                0.0,
                0.0,
                &payload(8),
                7,
                3,
                queue_index,
                &mut next_id,
            );
            (0..s.len())
                .map(|i| (s.id[i], s.px[i], s.py[i]))
                .collect::<Vec<_>>()
        };
        assert_eq!(make(0), make(0), "same inputs must reproduce exactly");
        assert_ne!(
            make(0),
            make(1),
            "same-tick impacts must draw independent streams"
        );
    }

    #[test]
    fn shock_is_bitwise_identical_across_store_layouts() {
        // The drain-time layout differs between an uninterrupted run
        // (canonical order) and a fresh resume (id-sorted order); the shock
        // must produce identical bits on both. Many particles with distinct
        // weights make an order-dependent weight sum diverge in low bits.
        let geom = GridGeom::new(100.0, 100.0, 10.0);
        let particles: Vec<(u64, f32, f32)> = (0..37)
            .map(|k| {
                (
                    k,
                    30.0 + (k as f32 * 1.37) % 40.0,
                    30.0 + (k as f32 * 2.11) % 40.0,
                )
            })
            .collect();
        let run = |order: &[usize]| {
            let mut s = ParticleStore::default();
            for &idx in order {
                let (id, px, py) = particles[idx];
                s.push(id, px, py, 0.0, 0.0, 1.0 + id as f32 * 0.1, 1.0, 0.0);
            }
            let mut next_id = 64;
            apply(
                &mut s,
                &geom,
                50.0,
                50.0,
                30.0,
                4.0,
                17.0,
                &payload(0),
                7,
                3,
                0,
                &mut next_id,
            );
            let mut out: Vec<(u64, u32, u32, u32)> = (0..s.len())
                .map(|i| {
                    (
                        s.id[i],
                        s.energy[i].to_bits(),
                        s.vx[i].to_bits(),
                        s.vy[i].to_bits(),
                    )
                })
                .collect();
            out.sort_unstable_by_key(|&(id, ..)| id);
            out
        };
        let forward: Vec<usize> = (0..particles.len()).collect();
        let mut shuffled = forward.clone();
        shuffled.reverse();
        shuffled.swap(3, 20);
        shuffled.swap(7, 29);
        assert_eq!(
            run(&forward),
            run(&shuffled),
            "shock results must not depend on store layout order"
        );
    }

    #[test]
    fn rift_pushes_perpendicularly_with_falloff() {
        // Horizontal segment from (40, 50) to (60, 50), radius 10.
        let geom = GridGeom::new(100.0, 100.0, 10.0);
        let mut s = ParticleStore::default();
        s.push(0, 50.0, 55.0, 0.0, 0.0, 1.0, 0.0, 0.0); // above mid, d=5
        s.push(1, 45.0, 42.0, 0.0, 0.0, 0.5, 0.0, 0.0); // below, d=8
        s.push(2, 68.0, 50.0, 0.0, 0.0, 1.0, 0.0, 0.0); // beyond east end, d=8
        s.push(3, 50.0, 75.0, 0.0, 0.0, 1.0, 0.0, 0.0); // out of radius
        s.canonicalize(&geom);
        let mut next_id = 4;
        apply_rift(
            &mut s,
            &geom,
            40.0,
            50.0,
            60.0,
            50.0,
            10.0,
            6.0,
            0.0,
            &payload(0),
            7,
            3,
            0,
            &mut next_id,
        );
        let at = |id: u64| s.id.iter().position(|&x| x == id).unwrap();
        // Above the segment: pushed straight up (southward +y here), w=0.5.
        let i0 = at(0);
        assert_eq!(s.vx[i0], 0.0);
        assert!((s.vy[i0] - 3.0).abs() < 1e-5, "vy = {}", s.vy[i0]);
        // Below: pushed straight down, w=0.2, mass 0.5 → 2.4.
        let i1 = at(1);
        assert_eq!(s.vx[i1], 0.0);
        assert!((s.vy[i1] + 2.4).abs() < 1e-5, "vy = {}", s.vy[i1]);
        // Beyond the east endpoint: pushed radially from the endpoint
        // (pure +x here), w=0.2 → dv = 1.2.
        let i2 = at(2);
        assert!((s.vx[i2] - 1.2).abs() < 1e-5, "vx = {}", s.vx[i2]);
        assert_eq!(s.vy[i2], 0.0);
        // Out of radius: untouched.
        let i3 = at(3);
        assert_eq!((s.vx[i3], s.vy[i3]), (0.0, 0.0));
    }

    #[test]
    fn rift_energy_deposits_exactly_and_is_lost_without_targets() {
        let geom = GridGeom::new(100.0, 100.0, 10.0);
        let mut s = ParticleStore::default();
        s.push(0, 50.0, 55.0, 0.0, 0.0, 1.0, 1.0, 0.0);
        s.push(1, 45.0, 42.0, 0.0, 0.0, 0.5, 1.0, 0.0);
        s.canonicalize(&geom);
        let mut next_id = 2;
        let before: f32 = s.energy.iter().sum();
        apply_rift(
            &mut s,
            &geom,
            40.0,
            50.0,
            60.0,
            50.0,
            10.0,
            0.0,
            12.0,
            &payload(0),
            7,
            3,
            0,
            &mut next_id,
        );
        let after: f32 = s.energy.iter().sum();
        assert!(
            (after - before - 12.0).abs() < 1e-4,
            "deposited {}",
            after - before
        );
        // Nobody near this one: energy vanishes entirely.
        let before: f32 = s.energy.iter().sum();
        apply_rift(
            &mut s,
            &geom,
            10.0,
            10.0,
            20.0,
            10.0,
            2.0,
            0.0,
            99.0,
            &payload(0),
            7,
            3,
            1,
            &mut next_id,
        );
        let after: f32 = s.energy.iter().sum();
        assert_eq!(before, after);
    }

    #[test]
    fn degenerate_rift_shocks_exactly_like_an_impact() {
        // Same world, same params, zero payload: a zero-length rift and a
        // point impact must produce bitwise-identical shocks.
        let run = |rift: bool| {
            let (mut s, geom) = world();
            let mut next_id = 4;
            if rift {
                apply_rift(
                    &mut s,
                    &geom,
                    50.0,
                    50.0,
                    50.0,
                    50.0,
                    10.0,
                    6.0,
                    12.0,
                    &payload(0),
                    7,
                    3,
                    0,
                    &mut next_id,
                );
            } else {
                apply(
                    &mut s,
                    &geom,
                    50.0,
                    50.0,
                    10.0,
                    6.0,
                    12.0,
                    &payload(0),
                    7,
                    3,
                    0,
                    &mut next_id,
                );
            }
            (0..s.len())
                .map(|i| {
                    (
                        s.id[i],
                        s.vx[i].to_bits(),
                        s.vy[i].to_bits(),
                        s.energy[i].to_bits(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(run(true), run(false));
    }

    #[test]
    fn rift_payload_spawns_along_the_segment() {
        let geom = GridGeom::new(100.0, 100.0, 10.0);
        let mut s = ParticleStore::default();
        s.push(0, 5.0, 5.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        s.canonicalize(&geom);
        let mut next_id = 1;
        let p = payload(64);
        apply_rift(
            &mut s,
            &geom,
            30.0,
            40.0,
            70.0,
            40.0,
            5.0,
            0.0,
            0.0,
            &p,
            7,
            3,
            0,
            &mut next_id,
        );
        assert_eq!(s.len(), 1 + 64);
        assert_eq!(next_id, 1 + 64);
        for i in 1..s.len() {
            // Distance to the segment y=40, x in [30, 70] stays within the
            // spread disc drawn around a segment point.
            let dx = (s.px[i] - s.px[i].clamp(30.0, 70.0)).abs();
            let dy = (s.py[i] - 40.0).abs();
            let d = (dx * dx + dy * dy).sqrt();
            assert!(d <= p.spread + 1e-4, "particle {i} off the rift: {d}");
            assert!(s.matter[i] >= p.matter.lo && s.matter[i] < p.matter.hi);
            let speed = (s.vx[i] * s.vx[i] + s.vy[i] * s.vy[i]).sqrt();
            assert!(speed >= p.speed.lo - 1e-4 && speed < p.speed.hi + 1e-4);
        }
        // Spawns actually spread along the segment, not clumped at one end.
        let min_x = (1..s.len()).map(|i| s.px[i]).fold(f32::MAX, f32::min);
        let max_x = (1..s.len()).map(|i| s.px[i]).fold(f32::MIN, f32::max);
        assert!(max_x - min_x > 20.0, "span {}", max_x - min_x);
    }

    #[test]
    fn rift_stream_is_deterministic_index_distinct_and_impact_disjoint() {
        let run = |queue_index: u64, rift: bool| {
            let (mut s, geom) = world();
            let mut next_id = 4;
            if rift {
                apply_rift(
                    &mut s,
                    &geom,
                    20.0,
                    30.0,
                    20.0,
                    30.0,
                    5.0,
                    0.0,
                    0.0,
                    &payload(8),
                    7,
                    3,
                    queue_index,
                    &mut next_id,
                );
            } else {
                apply(
                    &mut s,
                    &geom,
                    20.0,
                    30.0,
                    5.0,
                    0.0,
                    0.0,
                    &payload(8),
                    7,
                    3,
                    queue_index,
                    &mut next_id,
                );
            }
            (0..s.len())
                .map(|i| (s.id[i], s.px[i], s.py[i]))
                .collect::<Vec<_>>()
        };
        assert_eq!(run(0, true), run(0, true));
        assert_ne!(run(0, true), run(1, true), "queue index must matter");
        assert_ne!(
            run(0, true),
            run(0, false),
            "a rift and an impact at the same tick/index must draw \
             independent streams"
        );
    }

    #[test]
    fn rift_shock_is_bitwise_identical_across_store_layouts() {
        let geom = GridGeom::new(100.0, 100.0, 10.0);
        let particles: Vec<(u64, f32, f32)> = (0..37)
            .map(|k| {
                (
                    k,
                    30.0 + (k as f32 * 1.37) % 40.0,
                    30.0 + (k as f32 * 2.11) % 40.0,
                )
            })
            .collect();
        let run = |order: &[usize]| {
            let mut s = ParticleStore::default();
            for &idx in order {
                let (id, px, py) = particles[idx];
                s.push(id, px, py, 0.0, 0.0, 1.0 + id as f32 * 0.1, 1.0, 0.0);
            }
            let mut next_id = 64;
            apply_rift(
                &mut s,
                &geom,
                35.0,
                40.0,
                65.0,
                60.0,
                25.0,
                4.0,
                17.0,
                &payload(0),
                7,
                3,
                0,
                &mut next_id,
            );
            let mut out: Vec<(u64, u32, u32, u32)> = (0..s.len())
                .map(|i| {
                    (
                        s.id[i],
                        s.energy[i].to_bits(),
                        s.vx[i].to_bits(),
                        s.vy[i].to_bits(),
                    )
                })
                .collect();
            out.sort_unstable_by_key(|&(id, ..)| id);
            out
        };
        let forward: Vec<usize> = (0..particles.len()).collect();
        let mut shuffled = forward.clone();
        shuffled.reverse();
        shuffled.swap(3, 20);
        shuffled.swap(7, 29);
        assert_eq!(run(&forward), run(&shuffled));
    }

    #[test]
    fn rift_end_caps_are_symmetric_at_half_world_length() {
        // Regression (night review 2026-07-10): the folded torus offset
        // flipped sign for particles past the far end of a segment once
        // segment length + radius exceeded half the world, so the far
        // end-cap was silently skipped while the near one was shocked —
        // shipped rift.ron authors exactly a half-world segment. Segment
        // (10,50)→(60,50), length 50 = world/2, radius 8: particles 6
        // beyond each end must take the same endpoint shock.
        let geom = GridGeom::new(100.0, 100.0, 10.0);
        let mut s = ParticleStore::default();
        s.push(0, 4.0, 50.0, 0.0, 0.0, 1.0, 0.0, 0.0); // 6 before the start
        s.push(1, 66.0, 50.0, 0.0, 0.0, 1.0, 0.0, 0.0); // 6 past the far end
        s.canonicalize(&geom);
        let mut next_id = 2;
        apply_rift(
            &mut s,
            &geom,
            10.0,
            50.0,
            60.0,
            50.0,
            8.0,
            4.0,
            10.0,
            &payload(0),
            7,
            3,
            0,
            &mut next_id,
        );
        let at = |id: u64| s.id.iter().position(|&x| x == id).unwrap();
        let (i0, i1) = (at(0), at(1));
        // Both end-caps: w = 1 - 6/8 = 0.25, dv = 4*0.25/1 = 1, outward.
        assert!((s.vx[i0] + 1.0).abs() < 1e-5, "near cap vx = {}", s.vx[i0]);
        assert!((s.vx[i1] - 1.0).abs() < 1e-5, "far cap vx = {}", s.vx[i1]);
        assert_eq!(s.vy[i0], 0.0);
        assert_eq!(s.vy[i1], 0.0);
        // Symmetric weights split the energy evenly.
        assert!((s.energy[i0] - 5.0).abs() < 1e-4);
        assert!((s.energy[i1] - 5.0).abs() < 1e-4);
    }

    #[test]
    fn rift_longer_than_half_the_world_shocks_its_whole_length() {
        // A 70-long segment in a 100 world: a particle 5 past the far end
        // folds its offset to -25 and used to be measured against the
        // start; it must be struck like an endpoint impact.
        let geom = GridGeom::new(100.0, 100.0, 10.0);
        let mut s = ParticleStore::default();
        s.push(0, 75.0, 50.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        s.canonicalize(&geom);
        let mut next_id = 1;
        apply_rift(
            &mut s,
            &geom,
            0.0,
            50.0,
            70.0,
            50.0,
            10.0,
            2.0,
            0.0,
            &payload(0),
            7,
            3,
            0,
            &mut next_id,
        );
        // d = 5 from the far endpoint, w = 0.5, dv = 1 outward (+x).
        assert!((s.vx[0] - 1.0).abs() < 1e-5, "vx = {}", s.vx[0]);
        assert_eq!(s.vy[0], 0.0);
    }

    #[test]
    fn rift_authored_across_the_seam_shocks_across_it() {
        let geom = GridGeom::new(100.0, 100.0, 10.0);
        let mut s = ParticleStore::default();
        // Particle just west of the seam; segment authored from x=95 to
        // x=105 (its wrapped end is x=5) passing right through the seam.
        s.push(0, 98.0, 54.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        s.canonicalize(&geom);
        let mut next_id = 1;
        apply_rift(
            &mut s,
            &geom,
            95.0,
            50.0,
            105.0,
            50.0,
            10.0,
            5.0,
            0.0,
            &payload(0),
            7,
            3,
            0,
            &mut next_id,
        );
        // x=98 projects inside the segment (t=0.3), distance 4 straight
        // down in +y: pushed perpendicular, no x component.
        assert_eq!(s.vx[0], 0.0, "vx = {}", s.vx[0]);
        assert!(s.vy[0] > 0.0, "vy = {}", s.vy[0]);
    }

    #[test]
    fn impact_wraps_across_the_torus_seam() {
        let geom = GridGeom::new(100.0, 100.0, 10.0);
        let mut s = ParticleStore::default();
        s.push(0, 98.0, 50.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        s.canonicalize(&geom);
        let mut next_id = 1;
        // Impact at x=2: the particle at x=98 is 4 away across the seam.
        apply(
            &mut s,
            &geom,
            2.0,
            50.0,
            10.0,
            5.0,
            0.0,
            &payload(0),
            7,
            3,
            0,
            &mut next_id,
        );
        assert!(
            s.vx[0] < 0.0,
            "seam shock must push westward (outward across the wrap), got {}",
            s.vx[0]
        );
    }
}
