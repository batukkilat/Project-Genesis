//! Uniform spatial grid over the torus world.
//!
//! Cell size is at least the interaction radius, so all pairs within the
//! cutoff live in the 3x3 neighborhood of a particle's cell. The grid is pure
//! geometry — derived from config, never from history — so it is identical
//! across runs, saves, and thread counts.

use bevy_ecs::prelude::*;

#[derive(Resource, Debug, Clone, Copy)]
pub struct GridGeom {
    pub cols: u32,
    pub rows: u32,
    pub cell_w: f32,
    pub cell_h: f32,
    pub world_w: f32,
    pub world_h: f32,
}

impl GridGeom {
    /// `interaction_radius` becomes the minimum cell size. Config validation
    /// guarantees at least 3 cells per axis.
    pub fn new(world_w: f32, world_h: f32, interaction_radius: f32) -> Self {
        let cols = (world_w / interaction_radius).floor().max(1.0) as u32;
        let rows = (world_h / interaction_radius).floor().max(1.0) as u32;
        GridGeom {
            cols,
            rows,
            cell_w: world_w / cols as f32,
            cell_h: world_h / rows as f32,
            world_w,
            world_h,
        }
    }

    pub fn cell_count(&self) -> usize {
        self.cols as usize * self.rows as usize
    }

    /// Cell index of a wrapped position. Clamped for the float edge case
    /// where `x / cell_w` rounds up to `cols`.
    pub fn cell_of(&self, x: f32, y: f32) -> u32 {
        let cx = ((x / self.cell_w) as u32).min(self.cols - 1);
        let cy = ((y / self.cell_h) as u32).min(self.rows - 1);
        cy * self.cols + cx
    }

    /// The 3x3 wrapped neighborhood of a cell, in a fixed canonical order
    /// (row-major, dy then dx). Iteration order must never depend on thread
    /// count or history — force accumulation order derives from it.
    pub fn neighbors_of(&self, cell: u32) -> [u32; 9] {
        let cx = cell % self.cols;
        let cy = cell / self.cols;
        let mut out = [0u32; 9];
        let mut k = 0;
        for dy in [self.rows - 1, 0, 1] {
            let ny = (cy + dy) % self.rows;
            for dx in [self.cols - 1, 0, 1] {
                let nx = (cx + dx) % self.cols;
                out[k] = ny * self.cols + nx;
                k += 1;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_size_at_least_radius() {
        let g = GridGeom::new(100.0, 60.0, 8.0);
        assert_eq!(g.cols, 12);
        assert_eq!(g.rows, 7);
        assert!(g.cell_w >= 8.0);
        assert!(g.cell_h >= 8.0);
    }

    #[test]
    fn cell_of_bounds() {
        let g = GridGeom::new(100.0, 100.0, 10.0);
        assert_eq!(g.cell_of(0.0, 0.0), 0);
        assert_eq!(g.cell_of(99.999, 99.999), g.cols * g.rows - 1);
        // Never out of range even at the seam.
        let c = g.cell_of(99.999_999, 0.0);
        assert!(c < g.cols);
    }

    #[test]
    fn neighbors_wrap_and_are_distinct() {
        let g = GridGeom::new(30.0, 30.0, 10.0); // 3x3 grid — the minimum
        for cell in 0..g.cell_count() as u32 {
            let n = g.neighbors_of(cell);
            let mut sorted = n;
            sorted.sort_unstable();
            for w in sorted.windows(2) {
                assert_ne!(w[0], w[1], "duplicate neighbor cell — grid too small");
            }
        }
    }

    #[test]
    fn neighbors_of_corner_wrap() {
        let g = GridGeom::new(50.0, 50.0, 10.0); // 5x5
        let n = g.neighbors_of(0); // top-left corner
        assert!(n.contains(&24), "must wrap to bottom-right corner");
    }
}
