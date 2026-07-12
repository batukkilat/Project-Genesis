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
use genesis_config::{EnvSpec, FieldDynamics};

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
    /// Per-field dynamics, index-aligned with `values` (Q-2026-07-08-C).
    /// All-static dynamics contribute nothing to the hash and skip the env
    /// step entirely.
    pub dynamics: Vec<FieldDynamics>,
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
            dynamics: spec.fields.iter().map(|f| f.dynamics).collect(),
        }
    }

    /// Rebuild from snapshot parts (load path). Grid cell sizes are derived
    /// from the world dims, exactly as at creation.
    pub fn from_parts(
        cols: u32,
        rows: u32,
        values: Vec<Vec<f32>>,
        dynamics: Vec<FieldDynamics>,
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
            dynamics,
        }
    }

    /// True when any field evolves on its own — the env step is skipped (and
    /// the dynamics params stay out of replay identity) otherwise.
    pub fn any_dynamic(&self) -> bool {
        self.dynamics.iter().any(|d| !d.is_static())
    }

    /// One tick of field dynamics (Q-2026-07-08-C): explicit 4-neighbor
    /// torus-Laplacian diffusion, then relaxation toward the rest value.
    /// Single-threaded — the env grid is tiny and a fixed evaluation order
    /// makes determinism trivial. Static fields are skipped whole, leaving
    /// their cells untouched bits.
    pub fn step(&mut self, dt: f32) {
        let (cols, rows) = (self.cols as usize, self.rows as usize);
        for (k, d) in self.dynamics.iter().enumerate() {
            if d.is_static() {
                continue;
            }
            let grid = &mut self.values[k];
            if d.diffusion > 0.0 {
                let r = d.diffusion * dt;
                let old = grid.clone();
                for cy in 0..rows {
                    let up = (cy + rows - 1) % rows;
                    let down = (cy + 1) % rows;
                    for cx in 0..cols {
                        let left = (cx + cols - 1) % cols;
                        let right = (cx + 1) % cols;
                        let lap = old[cy * cols + left]
                            + old[cy * cols + right]
                            + old[up * cols + cx]
                            + old[down * cols + cx]
                            - 4.0 * old[cy * cols + cx];
                        grid[cy * cols + cx] = old[cy * cols + cx] + r * lap;
                    }
                }
            }
            if d.relax_rate > 0.0 {
                let r = d.relax_rate * dt;
                for v in grid.iter_mut() {
                    *v += r * (d.relax_to - *v);
                }
            }
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

    /// Apply one player action (Q-2026-07-08-B): edit every cell whose
    /// center falls inside the action's region. Field indices are validated
    /// at simulation assembly. Single-threaded, called from the start-of-tick
    /// drain — cell iteration order is fixed, and set/add on disjoint cells
    /// is order-free anyway.
    pub fn apply_action(&mut self, kind: &genesis_config::ActionKind) {
        match *kind {
            genesis_config::ActionKind::FieldSet {
                field,
                region,
                value,
            } => self.edit(field, &region, |_| value),
            genesis_config::ActionKind::FieldAdd {
                field,
                region,
                delta,
            } => self.edit(field, &region, |v| v + delta),
            genesis_config::ActionKind::Impact { .. }
            | genesis_config::ActionKind::Rift { .. }
            | genesis_config::ActionKind::SpinSet { .. } => {
                unreachable!(
                    "impacts/rifts touch particles and spin touches physics params, \
                     not env fields — routed in the drain"
                )
            }
        }
    }

    fn edit(&mut self, field: u32, region: &genesis_config::RegionSpec, f: impl Fn(f32) -> f32) {
        let grid = &mut self.values[field as usize];
        for cy in 0..self.rows {
            let y = (cy as f32 + 0.5) * self.cell_h;
            for cx in 0..self.cols {
                let x = (cx as f32 + 0.5) * self.cell_w;
                if region.contains(x, y) {
                    let cell = (cy * self.cols + cx) as usize;
                    grid[cell] = f(grid[cell]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use genesis_config::FieldDynamics;
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
                    dynamics: Default::default(),
                })
                .collect(),
        }
    }

    fn dyn_spec(cols: u32, rows: u32, init: FieldInit, dynamics: FieldDynamics) -> EnvSpec {
        EnvSpec {
            cols,
            rows,
            fields: vec![EnvFieldSpec {
                name: String::new(),
                init,
                dynamics,
            }],
        }
    }

    #[test]
    fn diffusion_spreads_and_conserves_the_total() {
        // A single hot cell on a uniform-zero field: diffusion must move
        // value into the 4 neighbors and keep the field total constant
        // (every Laplacian flow is antisymmetric on the torus).
        let mut env = EnvFields::from_spec(
            &dyn_spec(
                8,
                8,
                FieldInit::Uniform(0.0),
                FieldDynamics {
                    diffusion: 1.0,
                    relax_rate: 0.0,
                    relax_to: 0.0,
                },
            ),
            64.0,
            64.0,
        );
        let center = 3 * 8 + 3;
        env.values[0][center] = 16.0;
        let total_before: f64 = env.values[0].iter().map(|&v| v as f64).sum();
        for _ in 0..10 {
            env.step(0.1); // rate * dt = 0.1, well inside stability
        }
        let grid = &env.values[0];
        assert!(
            grid[center] < 16.0,
            "spike must flatten, still {}",
            grid[center]
        );
        assert!(
            grid[center - 1] > 0.0 && grid[center + 1] > 0.0,
            "neighbors must have received value"
        );
        let total_after: f64 = grid.iter().map(|&v| v as f64).sum();
        assert!(
            (total_after - total_before).abs() < 1e-3,
            "diffusion leaked: {total_before} -> {total_after}"
        );
    }

    #[test]
    fn relax_approaches_the_rest_value() {
        let mut env = EnvFields::from_spec(
            &dyn_spec(
                4,
                4,
                FieldInit::Uniform(0.0),
                FieldDynamics {
                    diffusion: 0.0,
                    relax_rate: 1.0,
                    relax_to: 2.0,
                },
            ),
            64.0,
            64.0,
        );
        for _ in 0..100 {
            env.step(0.1);
        }
        for &v in &env.values[0] {
            assert!(
                (v - 2.0).abs() < 0.01,
                "field should have relaxed to 2.0, got {v}"
            );
        }
    }

    #[test]
    fn static_fields_are_untouched_bits() {
        // Mixed env: field 0 static, field 1 diffusing. The step must leave
        // field 0's cells bit-identical while field 1 evolves.
        let mixed = EnvSpec {
            cols: 4,
            rows: 4,
            fields: vec![
                EnvFieldSpec {
                    name: String::new(),
                    init: FieldInit::GradientX { lo: 0.0, hi: 1.0 },
                    dynamics: FieldDynamics::default(),
                },
                EnvFieldSpec {
                    name: String::new(),
                    init: FieldInit::GradientY { lo: 0.0, hi: 1.0 },
                    dynamics: FieldDynamics {
                        diffusion: 1.0,
                        relax_rate: 0.0,
                        relax_to: 0.0,
                    },
                },
            ],
        };
        let mut env = EnvFields::from_spec(&mixed, 64.0, 64.0);
        assert!(env.any_dynamic());
        let static_before = env.values[0].clone();
        let dynamic_before = env.values[1].clone();
        for _ in 0..5 {
            env.step(0.1);
        }
        assert_eq!(env.values[0], static_before, "static field was touched");
        assert_ne!(env.values[1], dynamic_before, "dynamic field never moved");
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
    fn apply_action_edits_cells_by_center() {
        use genesis_config::{ActionKind, RegionSpec};
        // 4x4 grid over 64x64: cell centers at 8, 24, 40, 56.
        let mut env = EnvFields::from_spec(&spec(4, 4, vec![FieldInit::Uniform(1.0)]), 64.0, 64.0);
        // Region covering centers 8 and 24 in x, 8 in y (half-open at 25).
        env.apply_action(&ActionKind::FieldSet {
            field: 0,
            region: RegionSpec {
                x0: 0.0,
                y0: 0.0,
                x1: 25.0,
                y1: 9.0,
            },
            value: 7.0,
        });
        assert_eq!(env.values[0][0], 7.0);
        assert_eq!(env.values[0][1], 7.0);
        assert_eq!(env.values[0][2], 1.0, "center 40 is outside x1=25");
        assert_eq!(env.values[0][4], 1.0, "center y=24 is outside y1=9");

        env.apply_action(&ActionKind::FieldAdd {
            field: 0,
            region: RegionSpec {
                x0: 0.0,
                y0: 0.0,
                x1: 9.0,
                y1: 9.0,
            },
            delta: -2.5,
        });
        assert_eq!(env.values[0][0], 4.5, "add composes on the set value");
        assert_eq!(env.values[0][1], 7.0, "outside the add region");
    }

    #[test]
    fn from_parts_matches_from_spec() {
        let world = (128.0, 64.0);
        let a = EnvFields::from_spec(
            &spec(8, 4, vec![FieldInit::GradientX { lo: 0.0, hi: 1.0 }]),
            world.0,
            world.1,
        );
        let b = EnvFields::from_parts(8, 4, a.values.clone(), a.dynamics.clone(), world.0, world.1);
        assert_eq!(a, b);
    }
}
