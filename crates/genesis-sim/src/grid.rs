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

    /// Number of chunk columns for a given chunk size (cells per chunk axis).
    /// A chunk is a `chunk_cells × chunk_cells` block of grid cells; the last
    /// chunk in a row or column may be partial when `cols`/`rows` is not a
    /// multiple of `chunk_cells`. Chunks are LOD geometry only — they never
    /// wrap and never own state, so a partial edge chunk is fine.
    pub fn chunk_cols(&self, chunk_cells: u32) -> u32 {
        self.cols.div_ceil(chunk_cells.max(1))
    }

    /// Number of chunk rows for a given chunk size. See [`Self::chunk_cols`].
    pub fn chunk_rows(&self, chunk_cells: u32) -> u32 {
        self.rows.div_ceil(chunk_cells.max(1))
    }

    /// Total chunk count for a given chunk size.
    pub fn chunk_count(&self, chunk_cells: u32) -> usize {
        self.chunk_cols(chunk_cells) as usize * self.chunk_rows(chunk_cells) as usize
    }

    /// Chunk index of a grid cell: which `chunk_cells × chunk_cells` block the
    /// cell falls in, row-major over chunks. Pure geometry — a function of the
    /// cell alone, identical across runs, saves, and thread counts.
    pub fn chunk_of(&self, cell: u32, chunk_cells: u32) -> u32 {
        let chunk_cells = chunk_cells.max(1);
        let cx = cell % self.cols;
        let cy = cell / self.cols;
        let chx = cx / chunk_cells;
        let chy = cy / chunk_cells;
        chy * self.chunk_cols(chunk_cells) + chx
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

    #[test]
    fn chunk_geometry_exact_multiple() {
        let g = GridGeom::new(80.0, 80.0, 10.0); // 8x8 grid
        assert_eq!(g.cols, 8);
        assert_eq!(g.rows, 8);
        // 2x2 chunks over an 8x8 grid → 4x4 = 16 chunks.
        assert_eq!(g.chunk_cols(2), 4);
        assert_eq!(g.chunk_rows(2), 4);
        assert_eq!(g.chunk_count(2), 16);
        // Cell (0,0) → chunk 0; cell (2,0) → chunk 1; cell (0,2) → chunk 4.
        assert_eq!(g.chunk_of(g.cell_of(1.0, 1.0), 2), 0);
        assert_eq!(g.chunk_of(g.cell_of(21.0, 1.0), 2), 1);
        assert_eq!(g.chunk_of(g.cell_of(1.0, 21.0), 2), 4);
    }

    #[test]
    fn chunk_geometry_partial_edge() {
        let g = GridGeom::new(50.0, 50.0, 10.0); // 5x5 grid
        // 2x2 chunks over 5 cells → ceil(5/2) = 3 chunk cols/rows → 9 chunks.
        assert_eq!(g.chunk_cols(2), 3);
        assert_eq!(g.chunk_rows(2), 3);
        assert_eq!(g.chunk_count(2), 9);
        // The last cell (4,4) falls in the partial corner chunk (2,2) = 8.
        let last = g.cols * g.rows - 1;
        assert_eq!(g.chunk_of(last, 2), 8);
    }

    #[test]
    fn every_cell_maps_into_range() {
        let g = GridGeom::new(70.0, 40.0, 10.0); // 7x4 grid
        for chunk_cells in [1u32, 2, 3, 4, 7, 16] {
            let n = g.chunk_count(chunk_cells) as u32;
            for cell in 0..g.cell_count() as u32 {
                assert!(
                    g.chunk_of(cell, chunk_cells) < n,
                    "cell {cell} chunk out of range at chunk_cells={chunk_cells}"
                );
            }
        }
    }

    #[test]
    fn chunk_cells_one_is_identity() {
        let g = GridGeom::new(60.0, 30.0, 10.0); // 6x3 grid
        assert_eq!(g.chunk_count(1), g.cell_count());
        for cell in 0..g.cell_count() as u32 {
            assert_eq!(g.chunk_of(cell, 1), cell, "chunk_cells=1 must be identity");
        }
    }
}
