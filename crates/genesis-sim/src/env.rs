//! Environment fields: generic indexed scalar fields on their own coarse
//! torus-aligned grid (Q-2026-07-08-A, docs/research/environment-fields.md).
//!
//! Fields are simulation *state* particles can read but never write directly:
//! rule conditions gate on sampled values (v10), and later work items add
//! field dynamics, physics coupling, and replay-recorded player edits. The
//! grid is deliberately independent of the interaction grid and LOD chunks —
//! no tuning knob shapes the universe.
//!
//! Determinism: with no dynamics the fields are constant; sampling is a pure
//! function of (position, grid dims), identical across runs, saves, and
//! thread counts. Replay identity follows the LOD precedent: zero fields
//! contribute nothing to the state hash, so every pre-env run keeps its
//! exact identity; declared fields hash grid dims + every cell value.

use bevy_ecs::prelude::*;
use genesis_config::EnvSpec;

/// All environment fields, SoA: `values[k]` is field `k`'s row-major cell
/// grid. Empty `values` means no environment (nothing stored, hashed, or
/// sampled).
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct EnvFields {
    pub cols: u32,
    pub rows: u32,
    cell_w: f32,
    cell_h: f32,
    /// One row-major `cols * rows` grid per field.
    pub values: Vec<Vec<f32>>,
}

impl EnvFields {
    /// Build from config at world creation: every init spec is evaluated at
    /// each env-cell center, then never consulted again — the cell values are
    /// the state.
    pub fn from_spec(spec: &EnvSpec, world_w: f32, world_h: f32) -> Self {
        if spec.fields.is_empty() {
            return EnvFields::default();
        }
        let (cols, rows) = (spec.cols, spec.rows);
        let values = spec
            .fields
            .iter()
            .map(|f| {
                let mut grid = Vec::with_capacity(cols as usize * rows as usize);
                for cy in 0..rows {
                    // Normalized cell-center coordinates in [0, 1).
                    let v = (cy as f32 + 0.5) / rows as f32;
                    for cx in 0..cols {
                        let u = (cx as f32 + 0.5) / cols as f32;
                        grid.push(f.init.value_at(u, v));
                    }
                }
                grid
            })
            .collect();
        EnvFields {
            cols,
            rows,
            cell_w: world_w / cols as f32,
            cell_h: world_h / rows as f32,
            values,
        }
    }

    /// Rebuild from snapshot parts (load path). Grid cell sizes are derived
    /// from the world dims, exactly as at creation.
    pub fn from_parts(
        cols: u32,
        rows: u32,
        values: Vec<Vec<f32>>,
        world_w: f32,
        world_h: f32,
    ) -> Self {
        if values.is_empty() {
            return EnvFields::default();
        }
        EnvFields {
            cols,
            rows,
            cell_w: world_w / cols as f32,
            cell_h: world_h / rows as f32,
            values,
        }
    }

    pub fn field_count(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Env cell index of a wrapped world position. Nearest-cell (the cell the
    /// position falls in), clamped for the float edge case where division
    /// rounds up to the grid dimension — same posture as `GridGeom::cell_of`.
    pub fn cell_of(&self, x: f32, y: f32) -> u32 {
        let cx = ((x / self.cell_w) as u32).min(self.cols - 1);
        let cy = ((y / self.cell_h) as u32).min(self.rows - 1);
        cy * self.cols + cx
    }

    /// Field `k` sampled at a wrapped world position.
    pub fn sample(&self, k: usize, x: f32, y: f32) -> f32 {
        self.values[k][self.cell_of(x, y) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use genesis_config::{EnvFieldSpec, FieldInit};

    fn spec(cols: u32, rows: u32, inits: Vec<FieldInit>) -> EnvSpec {
        EnvSpec {
            cols,
            rows,
            fields: inits
                .into_iter()
                .map(|init| EnvFieldSpec {
                    name: String::new(),
                    init,
                })
                .collect(),
        }
    }

    #[test]
    fn empty_spec_builds_empty_fields() {
        let env = EnvFields::from_spec(&EnvSpec::default(), 256.0, 256.0);
        assert!(env.is_empty());
        assert_eq!(env.field_count(), 0);
    }

    #[test]
    fn uniform_field_is_uniform() {
        let env = EnvFields::from_spec(&spec(8, 4, vec![FieldInit::Uniform(2.5)]), 128.0, 64.0);
        assert_eq!(env.field_count(), 1);
        assert_eq!(env.values[0].len(), 32);
        assert!(env.values[0].iter().all(|&v| v == 2.5));
        assert_eq!(env.sample(0, 0.0, 0.0), 2.5);
        assert_eq!(env.sample(0, 127.9, 63.9), 2.5);
    }

    #[test]
    fn gradient_x_ramps_west_to_east() {
        let env = EnvFields::from_spec(
            &spec(4, 1, vec![FieldInit::GradientX { lo: 0.0, hi: 8.0 }]),
            64.0,
            16.0,
        );
        // Cell centers at u = 1/8, 3/8, 5/8, 7/8 → values 1, 3, 5, 7.
        assert_eq!(env.values[0], vec![1.0, 3.0, 5.0, 7.0]);
        // Sampling picks the containing cell.
        assert_eq!(env.sample(0, 2.0, 8.0), 1.0);
        assert_eq!(env.sample(0, 63.0, 8.0), 7.0);
    }

    #[test]
    fn gradient_y_ramps_north_to_south() {
        let env = EnvFields::from_spec(
            &spec(1, 4, vec![FieldInit::GradientY { lo: -1.0, hi: 1.0 }]),
            16.0,
            64.0,
        );
        assert_eq!(env.values[0], vec![-0.75, -0.25, 0.25, 0.75]);
        assert_eq!(env.sample(0, 8.0, 0.0), -0.75);
        assert_eq!(env.sample(0, 8.0, 63.9), 0.75);
    }

    #[test]
    fn multiple_fields_are_independent() {
        let env = EnvFields::from_spec(
            &spec(
                4,
                4,
                vec![
                    FieldInit::Uniform(1.0),
                    FieldInit::GradientX { lo: 0.0, hi: 4.0 },
                ],
            ),
            64.0,
            64.0,
        );
        assert_eq!(env.field_count(), 2);
        assert_ne!(env.values[0], env.values[1]);
    }

    #[test]
    fn cell_of_never_exceeds_grid() {
        let env = EnvFields::from_spec(&spec(3, 3, vec![FieldInit::Uniform(0.0)]), 100.0, 100.0);
        // Positions at or just below the world edge stay in range even when
        // x / cell_w rounds up.
        let c = env.cell_of(99.999_999, 99.999_999);
        assert!(c < 9);
    }

    #[test]
    fn from_parts_matches_from_spec() {
        let world = (128.0, 64.0);
        let a = EnvFields::from_spec(
            &spec(8, 4, vec![FieldInit::GradientX { lo: 0.0, hi: 1.0 }]),
            world.0,
            world.1,
        );
        let b = EnvFields::from_parts(8, 4, a.values.clone(), world.0, world.1);
        assert_eq!(a, b);
    }
}
