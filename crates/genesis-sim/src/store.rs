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
    /// Offsets into the particle arrays per cell (len = cell_count + 1);
    /// valid after `canonicalize`.
    pub cell_start: Vec<u32>,
    // Reusable scratch to avoid per-tick allocations.
    order: Vec<u32>,
    scratch_f32: Vec<f32>,
    scratch_u64: Vec<u64>,
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
    }

    /// Re-sort all particle arrays into (cell, id) order and rebuild the
    /// per-cell offsets. Must run before the force pass each tick.
    pub fn canonicalize(&mut self, geom: &GridGeom) {
        let n = self.len();

        self.cell.clear();
        self.cell
            .extend((0..n).map(|i| geom.cell_of(self.px[i], self.py[i])));

        self.order.clear();
        self.order.extend(0..n as u32);
        let cell = &self.cell;
        let id = &self.id;
        self.order.par_sort_unstable_by_key(|&i| {
            ((cell[i as usize] as u128) << 64) | id[i as usize] as u128
        });

        let order = std::mem::take(&mut self.order);
        permute_u64(&mut self.id, &order, &mut self.scratch_u64);
        permute_f32(&mut self.px, &order, &mut self.scratch_f32);
        permute_f32(&mut self.py, &order, &mut self.scratch_f32);
        permute_f32(&mut self.vx, &order, &mut self.scratch_f32);
        permute_f32(&mut self.vy, &order, &mut self.scratch_f32);
        permute_f32(&mut self.matter, &order, &mut self.scratch_f32);
        permute_f32(&mut self.energy, &order, &mut self.scratch_f32);
        permute_f32(&mut self.information, &order, &mut self.scratch_f32);
        {
            // `cell` is rebuilt cheaply from the permutation as well.
            let mut scratch = std::mem::take(&mut self.scratch_u64);
            scratch.clear();
            scratch.extend(order.iter().map(|&i| self.cell[i as usize] as u64));
            self.cell.clear();
            self.cell.extend(scratch.iter().map(|&c| c as u32));
            self.scratch_u64 = scratch;
        }
        self.order = order;

        self.cell_start.clear();
        self.cell_start.resize(geom.cell_count() + 1, 0);
        for &c in &self.cell {
            self.cell_start[c as usize + 1] += 1;
        }
        for i in 1..self.cell_start.len() {
            self.cell_start[i] += self.cell_start[i - 1];
        }

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
