//! SoA particle storage, kept in canonical (cell, id) order.
//!
//! Design notes for determinism:
//!
//! - Every tick starts by re-sorting particles into (cell, id) order. The
//!   layout is therefore a pure function of the current state — not of
//!   history, thread count, or save/load boundaries. Force accumulation
//!   order derives from the layout, so replay and resume stay bit-identical.
//! - Parallel passes only ever write to disjoint per-particle slots; there
//!   are no cross-thread reductions. Thread count cannot affect results.

use bevy_ecs::prelude::*;
use rayon::prelude::*;

use crate::grid::GridGeom;

/// Chunk length for parallel iteration. Purely a scheduling knob — results
/// are identical for any value.
const PAR_CHUNK: usize = 4096;

#[derive(Resource, Debug, Default)]
pub struct ParticleStore {
    pub id: Vec<u64>,
    pub px: Vec<f32>,
    pub py: Vec<f32>,
    pub vx: Vec<f32>,
    pub vy: Vec<f32>,
    pub matter: Vec<f32>,
    pub energy: Vec<f32>,
    pub information: Vec<f32>,
    /// Cell index per particle; valid after `canonicalize`.
    pub cell: Vec<u32>,
    /// Force accumulators; valid after the force pass.
    pub fx: Vec<f32>,
    pub fy: Vec<f32>,
    /// Adaptive-detail activity mask; valid after the classify pass. An empty
    /// vector means "not tracked" — every particle is treated as active, which
    /// is how the physics/interaction unit tests (which never classify) and a
    /// disabled LOD policy both behave. Kept index-aligned with the particle
    /// arrays through `remove_dead` and same-tick emissions.
    pub active: Vec<bool>,
    /// Offsets into the particle arrays per cell (len = cell_count + 1);
    /// valid after `canonicalize`.
    pub cell_start: Vec<u32>,
    // Reusable scratch to avoid per-tick allocations.
    order: Vec<u32>,
    scratch_f32: Vec<f32>,
    scratch_u64: Vec<u64>,
    scratch_u32: Vec<u32>,
    new_cell: Vec<u32>,
    moved: Vec<u32>,
    /// Set by `push`/`remove_dead`: indices no longer line up with the layout
    /// the previous `canonicalize` left behind, so the incremental re-sort
    /// must not trust `cell` and falls back to the full sort.
    structure_dirty: bool,
}

impl ParticleStore {
    pub fn len(&self) -> usize {
        self.id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.id.is_empty()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push(&mut self, id: u64, px: f32, py: f32, vx: f32, vy: f32, m: f32, e: f32, i: f32) {
        self.structure_dirty = true;
        self.id.push(id);
        self.px.push(px);
        self.py.push(py);
        self.vx.push(vx);
        self.vy.push(vy);
        self.matter.push(m);
        self.energy.push(e);
        self.information.push(i);
    }

    /// Order-preserving removal of dead particles (`alive[i] == false`).
    /// `alive` may be shorter than the store — particles appended after it
    /// was sized (same-tick emissions) are always kept. Also compacts the
    /// force accumulators so a following `integrate` stays index-aligned;
    /// `cell`/`cell_start` go stale and are rebuilt by the next
    /// `canonicalize`.
    pub fn remove_dead(&mut self, alive: &[bool]) {
        self.structure_dirty = true;
        // The activity mask is compacted alongside only when it is tracked and
        // index-aligned (populated by the classify pass); an empty/short mask
        // means LOD is off, so leave it empty.
        let track_active = self.active.len() == self.len();
        let keep = |i: usize| alive.get(i).copied().unwrap_or(true);
        let mut w = 0;
        for r in 0..self.len() {
            if keep(r) {
                self.id[w] = self.id[r];
                self.px[w] = self.px[r];
                self.py[w] = self.py[r];
                self.vx[w] = self.vx[r];
                self.vy[w] = self.vy[r];
                self.matter[w] = self.matter[r];
                self.energy[w] = self.energy[r];
                self.information[w] = self.information[r];
                self.fx[w] = self.fx[r];
                self.fy[w] = self.fy[r];
                if track_active {
                    self.active[w] = self.active[r];
                }
                w += 1;
            }
        }
        self.id.truncate(w);
        self.px.truncate(w);
        self.py.truncate(w);
        self.vx.truncate(w);
        self.vy.truncate(w);
        self.matter.truncate(w);
        self.energy.truncate(w);
        self.information.truncate(w);
        self.fx.truncate(w);
        self.fy.truncate(w);
        if track_active {
            self.active.truncate(w);
        }
    }

    /// Re-sort all particle arrays into (cell, id) order and rebuild the
    /// per-cell offsets. Must run before the force pass each tick.
    ///
    /// The target layout is the unique ascending (cell, id) order — ids are
    /// unique — so every path below produces bit-identical output; they
    /// differ only in cost:
    ///
    /// - Incremental (`!structure_dirty`): the arrays are still sorted by
    ///   (previous cell, id), so particles whose cell is unchanged form an
    ///   already-sorted subsequence; only cell-changers are sorted and merged
    ///   back in. When nothing changed cell the whole layout is reused. This
    ///   is what makes the sort scale with *motion* rather than population —
    ///   the fixed cost that bounded the LOD speedup (BASELINES.md, Phase 4).
    /// - Full parallel sort: first tick, after create/destroy events, or when
    ///   a majority of the world changed cells.
    pub fn canonicalize(&mut self, geom: &GridGeom) {
        let n = self.len();

        // This tick's cells go into scratch: the previous tick's `cell` must
        // survive intact for the incremental comparison.
        let mut new_cell = std::mem::take(&mut self.new_cell);
        new_cell.clear();
        new_cell.extend((0..n).map(|i| geom.cell_of(self.px[i], self.py[i])));

        let mut order = std::mem::take(&mut self.order);
        order.clear();

        let mut sorted = false;
        if !self.structure_dirty && self.cell.len() == n {
            let mut moved = std::mem::take(&mut self.moved);
            moved.clear();
            moved.extend((0..n as u32).filter(|&i| new_cell[i as usize] != self.cell[i as usize]));
            if moved.is_empty() {
                // Layout unchanged: `cell` and `cell_start` stay valid as-is.
                // (`cell_start` can only be missing on a store that has never
                // canonicalized, i.e. the empty one.)
                if self.cell_start.len() != geom.cell_count() + 1 {
                    self.rebuild_cell_start(geom);
                }
                self.new_cell = new_cell;
                self.order = order;
                self.moved = moved;
                self.reset_forces(n);
                return;
            }
            // Merging pays while the moved set is a minority; past that the
            // full parallel sort wins. Same output either way.
            if moved.len() <= n / 2 {
                let id = &self.id;
                let key = |i: u32| ((new_cell[i as usize] as u128) << 64) | id[i as usize] as u128;
                moved.par_sort_unstable_by_key(|&i| key(i));

                // Two-pointer merge of the stable subsequence (ascending index
                // = ascending key, since unchanged keys kept last tick's
                // order) with the sorted moved list. Keys are unique, so the
                // result is the exact full-sort order.
                order.reserve(n);
                let mut mj = 0;
                for i in 0..n as u32 {
                    if new_cell[i as usize] != self.cell[i as usize] {
                        continue; // re-placed via the moved list
                    }
                    let k = key(i);
                    while mj < moved.len() && key(moved[mj]) < k {
                        order.push(moved[mj]);
                        mj += 1;
                    }
                    order.push(i);
                }
                order.extend_from_slice(&moved[mj..]);
                sorted = true;
            }
            self.moved = moved;
        }

        if !sorted {
            order.extend(0..n as u32);
            let id = &self.id;
            order.par_sort_unstable_by_key(|&i| {
                ((new_cell[i as usize] as u128) << 64) | id[i as usize] as u128
            });
        }

        permute_u64(&mut self.id, &order, &mut self.scratch_u64);
        permute_f32(&mut self.px, &order, &mut self.scratch_f32);
        permute_f32(&mut self.py, &order, &mut self.scratch_f32);
        permute_f32(&mut self.vx, &order, &mut self.scratch_f32);
        permute_f32(&mut self.vy, &order, &mut self.scratch_f32);
        permute_f32(&mut self.matter, &order, &mut self.scratch_f32);
        permute_f32(&mut self.energy, &order, &mut self.scratch_f32);
        permute_f32(&mut self.information, &order, &mut self.scratch_f32);
        // `cell` takes this tick's values, permuted into the new order.
        self.scratch_u32.clear();
        self.scratch_u32
            .extend(order.iter().map(|&i| new_cell[i as usize]));
        std::mem::swap(&mut self.cell, &mut self.scratch_u32);
        self.new_cell = new_cell;
        self.order = order;
        self.structure_dirty = false;

        self.rebuild_cell_start(geom);
        self.reset_forces(n);
    }

    fn rebuild_cell_start(&mut self, geom: &GridGeom) {
        self.cell_start.clear();
        self.cell_start.resize(geom.cell_count() + 1, 0);
        for &c in &self.cell {
            self.cell_start[c as usize + 1] += 1;
        }
        for i in 1..self.cell_start.len() {
            self.cell_start[i] += self.cell_start[i - 1];
        }
    }

    fn reset_forces(&mut self, n: usize) {
        self.fx.clear();
        self.fx.resize(n, 0.0);
        self.fy.clear();
        self.fy.resize(n, 0.0);
    }
}

/// Chunk length used by the parallel physics passes (see `physics.rs`).
/// Purely a scheduling knob — results are identical for any value.
pub const fn par_chunk() -> usize {
    PAR_CHUNK
}

fn permute_f32(data: &mut Vec<f32>, order: &[u32], scratch: &mut Vec<f32>) {
    scratch.clear();
    scratch.extend(order.iter().map(|&i| data[i as usize]));
    std::mem::swap(data, scratch);
}

fn permute_u64(data: &mut Vec<u64>, order: &[u32], scratch: &mut Vec<u64>) {
    scratch.clear();
    scratch.extend(order.iter().map(|&i| data[i as usize]));
    std::mem::swap(data, scratch);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalize_sorts_by_cell_then_id() {
        let geom = GridGeom::new(30.0, 30.0, 10.0);
        let mut s = ParticleStore::default();
        // Two particles in cell (2,2), one in cell (0,0) — inserted out of order.
        s.push(5, 25.0, 25.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        s.push(1, 2.0, 2.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        s.push(3, 26.0, 26.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        s.canonicalize(&geom);
        assert_eq!(s.id, vec![1, 3, 5]);
        assert_eq!(s.cell[0], 0);
        assert_eq!(s.cell[1], s.cell[2]);
        // cell_start brackets the last cell correctly.
        let last = geom.cell_count();
        assert_eq!(s.cell_start[last], 3);
    }

    /// Reference layout: push the store's current particles into a fresh
    /// store (always dirty → full sort) and canonicalize. Every incremental
    /// path must reproduce this exactly.
    fn full_sort_reference(s: &ParticleStore, geom: &GridGeom) -> ParticleStore {
        let mut r = ParticleStore::default();
        for i in 0..s.len() {
            r.push(
                s.id[i],
                s.px[i],
                s.py[i],
                s.vx[i],
                s.vy[i],
                s.matter[i],
                s.energy[i],
                s.information[i],
            );
        }
        r.canonicalize(geom);
        r
    }

    fn assert_same_layout(a: &ParticleStore, b: &ParticleStore, what: &str) {
        assert_eq!(a.id, b.id, "{what}: id order diverged");
        assert_eq!(a.px, b.px, "{what}: px diverged");
        assert_eq!(a.py, b.py, "{what}: py diverged");
        assert_eq!(a.vx, b.vx, "{what}: vx diverged");
        assert_eq!(a.vy, b.vy, "{what}: vy diverged");
        assert_eq!(a.matter, b.matter, "{what}: matter diverged");
        assert_eq!(a.energy, b.energy, "{what}: energy diverged");
        assert_eq!(a.information, b.information, "{what}: information diverged");
        assert_eq!(a.cell, b.cell, "{what}: cell diverged");
        assert_eq!(a.cell_start, b.cell_start, "{what}: cell_start diverged");
    }

    #[test]
    fn incremental_no_moves_keeps_layout_and_resets_forces() {
        let geom = GridGeom::new(30.0, 30.0, 10.0);
        let mut s = ParticleStore::default();
        s.push(2, 25.0, 25.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        s.push(1, 2.0, 2.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        s.canonicalize(&geom); // full sort (dirty after pushes)
        s.fx[0] = 7.0;
        s.fy[1] = -3.0;
        // Nothing moved: second canonicalize must take the skip path and
        // still leave a canonical layout with cleared force accumulators.
        s.canonicalize(&geom);
        assert_same_layout(&s, &full_sort_reference(&s, &geom), "no-move skip");
        assert_eq!(s.fx, vec![0.0, 0.0]);
        assert_eq!(s.fy, vec![0.0, 0.0]);
    }

    #[test]
    fn incremental_merge_matches_full_sort() {
        let geom = GridGeom::new(30.0, 30.0, 10.0);
        let mut s = ParticleStore::default();
        // Nine particles spread over the 3x3 grid.
        for k in 0..9u64 {
            let x = (k % 3) as f32 * 10.0 + 5.0;
            let y = (k / 3) as f32 * 10.0 + 5.0;
            s.push(k, x, y, 0.0, 0.0, 1.0, 0.5, 0.25);
        }
        s.canonicalize(&geom);
        // Move a minority across cell boundaries (merge path), including a
        // torus-seam crossing and a swap into an occupied cell.
        let a = s.id.iter().position(|&x| x == 0).unwrap();
        s.px[a] = 29.5; // cell (0,0) -> (2,0)
        let b = s.id.iter().position(|&x| x == 7).unwrap();
        s.py[b] = 0.5; // cell (1,2) -> (1,0)
        s.canonicalize(&geom);
        assert_same_layout(&s, &full_sort_reference(&s, &geom), "merge");
    }

    #[test]
    fn incremental_random_walk_always_matches_full_sort() {
        // Random-walk churn across many ticks exercises skip, merge, and
        // full-sort paths; after every canonicalize the layout must equal the
        // from-scratch reference. Deterministic LCG — no wall-clock entropy.
        let geom = GridGeom::new(50.0, 50.0, 10.0);
        let mut rng: u64 = 0x9E3779B97F4A7C15;
        let mut next = || {
            rng = rng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (rng >> 33) as f32 / (1u64 << 31) as f32 // [0, 1)
        };
        let mut s = ParticleStore::default();
        for k in 0..200u64 {
            s.push(k, next() * 50.0, next() * 50.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        }
        s.canonicalize(&geom);
        for step in 0..40 {
            // Step size varies: quiet ticks (few cross cells), violent ticks
            // (most do), and occasional create/destroy to trip the dirty flag.
            let scale = match step % 4 {
                0 => 0.0,  // nobody moves — skip path
                1 => 1.0,  // few cross — merge path
                _ => 20.0, // most cross — full path
            };
            for i in 0..s.len() {
                s.px[i] = (s.px[i] + (next() - 0.5) * scale).rem_euclid(50.0);
                s.py[i] = (s.py[i] + (next() - 0.5) * scale).rem_euclid(50.0);
            }
            if step % 7 == 3 {
                let mut alive = vec![true; s.len()];
                alive[step % s.len()] = false;
                s.fx.resize(s.len(), 0.0); // remove_dead compacts force slots
                s.fy.resize(s.len(), 0.0);
                s.remove_dead(&alive);
                s.push(
                    1000 + step as u64,
                    next() * 50.0,
                    next() * 50.0,
                    0.0,
                    0.0,
                    1.0,
                    0.0,
                    0.0,
                );
            }
            s.canonicalize(&geom);
            assert_same_layout(&s, &full_sort_reference(&s, &geom), &format!("step {step}"));
        }
    }

    #[test]
    fn canonicalize_is_state_pure() {
        // Same particle state inserted in two different orders must produce
        // identical layouts — this is what makes save/resume byte-identical.
        let geom = GridGeom::new(30.0, 30.0, 10.0);
        let mut a = ParticleStore::default();
        a.push(1, 2.0, 2.0, 0.5, 0.0, 1.0, 0.0, 0.0);
        a.push(2, 2.5, 2.0, -0.5, 0.0, 1.0, 0.0, 0.0);
        let mut b = ParticleStore::default();
        b.push(2, 2.5, 2.0, -0.5, 0.0, 1.0, 0.0, 0.0);
        b.push(1, 2.0, 2.0, 0.5, 0.0, 1.0, 0.0, 0.0);
        a.canonicalize(&geom);
        b.canonicalize(&geom);
        assert_eq!(a.id, b.id);
        assert_eq!(a.px, b.px);
        assert_eq!(a.vx, b.vx);
    }
}
