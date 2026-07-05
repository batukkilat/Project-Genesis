//! Continuous dynamics: the generic short-range pairwise kernel and
//! semi-implicit Euler integration on the torus.
//!
//! This is Layer 2 (Physics) — hardcoded generic operators, parameterized by
//! config, no Earth constants. Discrete data-driven events are Layer 1
//! (Interaction System, Phase 3), not here.
//!
//! Determinism: the force pass reads positions from the start of the tick and
//! writes only each particle's own force slot; per-particle neighbor
//! iteration order comes from the canonical (cell, id) layout. No RNG, no
//! cross-thread reductions — thread count cannot change a single bit.

use genesis_config::PhysicsParams;
use genesis_core::torus;
use rayon::prelude::*;

use crate::bonds::BondStore;
use crate::grid::GridGeom;
use crate::store::{ParticleStore, par_chunk};

/// Signed kernel magnitude at center distance `r`, in `[0, interaction_radius)`.
/// Positive = attraction (toward the other particle), negative = repulsion.
///
/// Shape: linear repulsion core in `[0, rc)`, triangular attraction band in
/// `[rc, R)` peaking at the middle, zero at and beyond `R`.
pub fn kernel(r: f32, p: &PhysicsParams) -> f32 {
    let rr = p.interaction_radius;
    let rc = p.core_frac * rr;
    if r < rc {
        -p.repulsion * (1.0 - r / rc)
    } else if r < rr {
        let mid = 0.5 * (rc + rr);
        let half = 0.5 * (rr - rc);
        p.attraction * (1.0 - (r - mid).abs() / half)
    } else {
        0.0
    }
}

/// Pair potential energy U(r) with U(R) = 0, satisfying dU/dr = kernel(r)
/// (kernel sign convention: positive pulls the pair together, which lowers U
/// as r shrinks). Used by conservation tests; not on the hot path.
pub fn potential(r: f32, p: &PhysicsParams) -> f32 {
    let rr = p.interaction_radius;
    let rc = p.core_frac * rr;
    let mid = 0.5 * (rc + rr);
    let half = 0.5 * (rr - rc);
    if r >= rr {
        0.0
    } else if r >= mid {
        let u = r - mid;
        -p.attraction * ((half - u) - (half * half - u * u) / (2.0 * half))
    } else if r >= rc {
        let u_mid = -p.attraction * (half - half / 2.0);
        let w = mid - r;
        u_mid - p.attraction * (w - w * w / (2.0 * half))
    } else {
        let u_rc = -p.attraction * half;
        let d = rc - r;
        u_rc + p.repulsion * d * d / (2.0 * rc)
    }
}

/// Force pass: for every particle, accumulate the kernel force from all
/// neighbors within the cutoff, plus the spring force from its bonds. Store
/// must be canonicalized and `bonds` rebuilt against it first.
///
/// Bond springs are harmonic: magnitude `strength * (r - rest_length)`,
/// pulling toward the partner when stretched and pushing when compressed.
/// Each endpoint computes its own side from the CSR mirror (disjoint
/// writes); `torus::delta` antisymmetry makes the pair forces exactly
/// opposite, so momentum is conserved. Unlike the kernel, bond springs have
/// no distance cutoff — a bond keeps pulling however far it stretches
/// (torus wrap bounds this at half the world).
pub fn forces(
    store: &mut ParticleStore,
    geom: &GridGeom,
    params: &PhysicsParams,
    bonds: &BondStore,
) {
    let ParticleStore {
        px,
        py,
        cell,
        cell_start,
        fx,
        fy,
        ..
    } = store;
    let px: &[f32] = px;
    let py: &[f32] = py;
    let cell: &[u32] = cell;
    let cell_start: &[u32] = cell_start;

    let r_cut2 = params.interaction_radius * params.interaction_radius;
    let world = (geom.world_w, geom.world_h);
    let rest = params.bond_rest_length;
    let has_bonds = !bonds.is_empty();

    fx.par_chunks_mut(par_chunk())
        .zip(fy.par_chunks_mut(par_chunk()))
        .enumerate()
        .for_each(|(chunk_idx, (fx_chunk, fy_chunk))| {
            let base = chunk_idx * par_chunk();
            for (k, (fx_i, fy_i)) in fx_chunk.iter_mut().zip(fy_chunk.iter_mut()).enumerate() {
                let i = base + k;
                let (xi, yi) = (px[i], py[i]);
                let mut ax = 0.0f32;
                let mut ay = 0.0f32;
                for &nc in geom.neighbors_of(cell[i]).iter() {
                    let start = cell_start[nc as usize] as usize;
                    let end = cell_start[nc as usize + 1] as usize;
                    for j in start..end {
                        if j == i {
                            continue;
                        }
                        let dx = torus::delta(xi, px[j], world.0);
                        let dy = torus::delta(yi, py[j], world.1);
                        let r2 = dx * dx + dy * dy;
                        if r2 >= r_cut2 || r2 == 0.0 {
                            continue;
                        }
                        let r = r2.sqrt();
                        let f = kernel(r, params) / r;
                        ax += dx * f;
                        ay += dy * f;
                    }
                }
                if has_bonds {
                    for (partner, row) in bonds.partners_of(i) {
                        let j = partner as usize;
                        let dx = torus::delta(xi, px[j], world.0);
                        let dy = torus::delta(yi, py[j], world.1);
                        let r2 = dx * dx + dy * dy;
                        if r2 == 0.0 {
                            continue;
                        }
                        let r = r2.sqrt();
                        let f = bonds.strength[row as usize] * (r - rest) / r;
                        ax += dx * f;
                        ay += dy * f;
                    }
                }
                *fx_i = ax;
                *fy_i = ay;
            }
        });
}

/// Semi-implicit (symplectic) Euler: v += (F/m) dt, then x += v dt, wrapped.
/// Also applies information decay (`params.information_decay`, per second):
/// information is deliberately not conserved — copy actions create it, decay
/// destroys it (decisions log, 2026-07-05). A zero rate multiplies by
/// exactly 1.0 and is bit-neutral.
pub fn integrate(store: &mut ParticleStore, geom: &GridGeom, params: &PhysicsParams, dt: f32) {
    let ParticleStore {
        px,
        py,
        vx,
        vy,
        matter,
        information,
        fx,
        fy,
        ..
    } = store;
    let world_w = geom.world_w;
    let world_h = geom.world_h;
    // Config validation guarantees rate * dt <= 1, so the factor is in [0, 1].
    let decay_factor = 1.0 - params.information_decay * dt;

    px.par_chunks_mut(par_chunk())
        .zip(py.par_chunks_mut(par_chunk()))
        .zip(vx.par_chunks_mut(par_chunk()))
        .zip(vy.par_chunks_mut(par_chunk()))
        .zip(matter.par_chunks(par_chunk()))
        .zip(information.par_chunks_mut(par_chunk()))
        .zip(fx.par_chunks(par_chunk()))
        .zip(fy.par_chunks(par_chunk()))
        .for_each(
            |(((((((px_c, py_c), vx_c), vy_c), m_c), info_c), fx_c), fy_c)| {
                for k in 0..px_c.len() {
                    let inv_m = 1.0 / m_c[k];
                    vx_c[k] += fx_c[k] * inv_m * dt;
                    vy_c[k] += fy_c[k] * inv_m * dt;
                    px_c[k] = torus::wrap(px_c[k] + vx_c[k] * dt, world_w);
                    py_c[k] = torus::wrap(py_c[k] + vy_c[k] * dt, world_h);
                    if decay_factor != 1.0 {
                        info_c[k] *= decay_factor;
                    }
                }
            },
        );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> PhysicsParams {
        PhysicsParams {
            interaction_radius: 8.0,
            core_frac: 0.4,
            repulsion: 40.0,
            attraction: 5.0,
            bond_rest_length: 3.0,
            information_decay: 0.0,
        }
    }

    #[test]
    fn bond_spring_acts_beyond_kernel_cutoff() {
        // Two bonded particles farther apart than the kernel radius: the only
        // force is the spring, pulling them together with exactly opposite
        // forces (momentum conservation).
        let p = params();
        let geom = GridGeom::new(64.0, 64.0, p.interaction_radius);
        let mut s = ParticleStore::default();
        s.push(0, 20.0, 32.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        s.push(1, 36.0, 32.0, 0.0, 0.0, 1.0, 0.0, 0.0); // r = 16 > cutoff 8
        s.canonicalize(&geom);
        let mut bonds = BondStore::default();
        bonds.create(0, 1, 2.0);
        bonds.rebuild(&s);
        forces(&mut s, &geom, &p, &bonds);

        let i0 = s.id.iter().position(|&x| x == 0).unwrap();
        let i1 = s.id.iter().position(|&x| x == 1).unwrap();
        let expect = 2.0 * (16.0 - p.bond_rest_length); // strength * (r - rest)
        assert!((s.fx[i0] - expect).abs() < 1e-4, "got {}", s.fx[i0]);
        assert_eq!(s.fx[i0], -s.fx[i1], "pair forces must be exactly opposite");
        assert_eq!(s.fy[i0], 0.0);
        assert_eq!(s.fy[i1], 0.0);
    }

    #[test]
    fn compressed_bond_pushes_apart() {
        let p = params();
        let geom = GridGeom::new(64.0, 64.0, p.interaction_radius);
        let mut s = ParticleStore::default();
        // Separation 1.0 < rest 3.0 → spring pushes apart. Kernel repulsion
        // also pushes; check the spring contribution by differencing a
        // bondless run.
        s.push(0, 30.0, 32.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        s.push(1, 31.0, 32.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        s.canonicalize(&geom);
        forces(&mut s, &geom, &p, &BondStore::default());
        let i0 = s.id.iter().position(|&x| x == 0).unwrap();
        let bare = s.fx[i0];

        let mut bonds = BondStore::default();
        bonds.create(0, 1, 2.0);
        bonds.rebuild(&s);
        forces(&mut s, &geom, &p, &bonds);
        let spring = s.fx[i0] - bare;
        let expect = 2.0 * (1.0 - p.bond_rest_length); // negative: away from partner
        assert!(
            (spring - expect).abs() < 1e-4,
            "got {spring}, want {expect}"
        );
    }

    #[test]
    fn kernel_shape() {
        let p = params();
        let rc = p.core_frac * p.interaction_radius; // 3.2
        assert_eq!(kernel(0.0, &p), -p.repulsion);
        assert!(kernel(rc * 0.5, &p) < 0.0, "core is repulsive");
        let mid = 0.5 * (rc + p.interaction_radius);
        assert!(
            (kernel(mid, &p) - p.attraction).abs() < 1e-5,
            "band peak = attraction"
        );
        assert_eq!(kernel(p.interaction_radius, &p), 0.0);
        assert_eq!(kernel(100.0, &p), 0.0);
    }

    #[test]
    fn kernel_continuous_at_core_boundary() {
        let p = params();
        let rc = p.core_frac * p.interaction_radius;
        let below = kernel(rc - 1e-4, &p);
        let above = kernel(rc + 1e-4, &p);
        assert!(
            below.abs() < 0.01,
            "repulsion reaches ~0 at rc, got {below}"
        );
        assert!(
            above.abs() < 0.01,
            "attraction starts at ~0 at rc, got {above}"
        );
    }

    #[test]
    fn potential_matches_kernel_derivative() {
        // dU/dr == kernel(r), checked by central differences across all
        // three branches of the piecewise definition.
        let p = params();
        let h = 1e-3f32;
        for i in 1..800 {
            let r = i as f32 * 0.01; // 0.01 .. 8.0
            let du = (potential(r + h, &p) - potential(r - h, &p)) / (2.0 * h);
            let f = kernel(r, &p);
            assert!(
                (du - f).abs() < 0.05,
                "dU/dr = {du} but kernel = {f} at r = {r}"
            );
        }
    }

    #[test]
    fn potential_zero_at_cutoff() {
        let p = params();
        assert_eq!(potential(p.interaction_radius, &p), 0.0);
        assert_eq!(potential(50.0, &p), 0.0);
    }
}
