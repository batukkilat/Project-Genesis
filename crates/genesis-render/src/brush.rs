//! Field brush: a UI stroke → player actions.
//!
//! The second logic half of landing step 4 (docs/research/render-bootstrap.md,
//! with `pacer` as the first): the env tool the app shell wires to the mouse.
//! The UI is just another script author (Q-2026-07-08-B) — a brush stroke
//! becomes ordinary [`PlayerAction`] records fed through
//! `Simulation::queue_action`, the exact records a scripted `.ron` file would
//! contain. Nothing here touches simulation state.
//!
//! The v1 brush is an axis-aligned rectangle (half extents around the cursor),
//! because the settled action vocabulary edits axis-aligned world-coordinate
//! regions and "a seam-wrapping region is two rects" (Q-2026-07-08-B). A
//! stroke crossing one torus seam emits two actions, a corner stroke four —
//! the same shape a script author would write by hand.

use genesis_config::{ActionKind, PlayerAction, RegionSpec};

/// What the stroke does to the field — mirrors the two field verbs of the
/// action vocabulary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FieldOp {
    /// Set every covered cell to the value.
    Set(f32),
    /// Add the delta to every covered cell.
    Add(f32),
}

/// One brush stamp: cursor position in world coordinates plus half extents.
/// A drag is a sequence of stamps, one per input sample — each stamp is its
/// own action record, so recorded and scripted play stay one representation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrushStamp {
    pub x: f32,
    pub y: f32,
    pub half_w: f32,
    pub half_h: f32,
}

/// Split one wrapped axis span `[c - h, c + h)` into non-empty in-world
/// intervals. At most two: the unwrapped part first, then the wrapped part —
/// a deterministic order so identical strokes always emit identical scripts.
fn axis_intervals(center: f32, half: f32, world: f32) -> Vec<(f32, f32)> {
    if 2.0 * half >= world {
        // The brush covers the whole axis; one full-world interval.
        return vec![(0.0, world)];
    }
    let c = center.rem_euclid(world);
    let lo = c - half;
    let hi = c + half;
    let mut spans = Vec::new();
    if lo < 0.0 {
        spans.push((0.0, hi));
        spans.push((lo + world, world));
    } else if hi > world {
        spans.push((lo, world));
        spans.push((0.0, hi - world));
    } else {
        spans.push((lo, hi));
    }
    spans.retain(|&(a, b)| a < b);
    spans
}

/// The in-world rectangles one stamp covers: 1 away from the seams, 2
/// crossing one seam, 4 crossing a corner. Regions follow the action
/// vocabulary's half-open semantics (`x0 <= x < x1`); a stamp larger than
/// the world clamps to the whole world.
pub fn stamp_regions(stamp: BrushStamp, world_w: f32, world_h: f32) -> Vec<RegionSpec> {
    let xs = axis_intervals(stamp.x, stamp.half_w, world_w);
    let ys = axis_intervals(stamp.y, stamp.half_h, world_h);
    let mut regions = Vec::with_capacity(xs.len() * ys.len());
    for &(x0, x1) in &xs {
        for &(y0, y1) in &ys {
            regions.push(RegionSpec { x0, y0, x1, y1 });
        }
    }
    regions
}

/// Turn one stamp into tick-stamped player actions on `field` — one action
/// per seam piece, in the deterministic region order. Feed these to
/// `Simulation::queue_action` (live) or collect them into an `ActionScript`
/// (recorded play): the records are identical by construction.
pub fn stamp_actions(
    stamp: BrushStamp,
    field: u32,
    op: FieldOp,
    tick: u64,
    world_w: f32,
    world_h: f32,
) -> Vec<PlayerAction> {
    stamp_regions(stamp, world_w, world_h)
        .into_iter()
        .map(|region| PlayerAction {
            tick,
            action: match op {
                FieldOp::Set(value) => ActionKind::FieldSet {
                    field,
                    region,
                    value,
                },
                FieldOp::Add(delta) => ActionKind::FieldAdd {
                    field,
                    region,
                    delta,
                },
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use genesis_config::ActionScript;

    const W: f32 = 100.0;
    const H: f32 = 80.0;

    fn stamp(x: f32, y: f32, hw: f32, hh: f32) -> BrushStamp {
        BrushStamp {
            x,
            y,
            half_w: hw,
            half_h: hh,
        }
    }

    /// Ground truth: is the world point inside the stamp under torus
    /// wrapping? Half-open on the leading edge, like RegionSpec.
    fn in_stamp(px: f32, py: f32, s: BrushStamp) -> bool {
        let dx = (px - (s.x - s.half_w)).rem_euclid(W);
        let dy = (py - (s.y - s.half_h)).rem_euclid(H);
        (dx < 2.0 * s.half_w || 2.0 * s.half_w >= W) && (dy < 2.0 * s.half_h || 2.0 * s.half_h >= H)
    }

    #[test]
    fn interior_stamp_is_one_rect() {
        let regions = stamp_regions(stamp(50.0, 40.0, 10.0, 5.0), W, H);
        assert_eq!(
            regions,
            vec![RegionSpec {
                x0: 40.0,
                y0: 35.0,
                x1: 60.0,
                y1: 45.0
            }]
        );
    }

    #[test]
    fn seam_crossing_splits_in_two() {
        // Left seam.
        let regions = stamp_regions(stamp(2.0, 40.0, 5.0, 5.0), W, H);
        assert_eq!(regions.len(), 2);
        // Unwrapped piece first (deterministic script order).
        assert_eq!(regions[0].x0, 0.0);
        assert_eq!(regions[0].x1, 7.0);
        assert_eq!(regions[1].x0, 97.0);
        assert_eq!(regions[1].x1, 100.0);
        // Right seam.
        let regions = stamp_regions(stamp(99.0, 40.0, 5.0, 5.0), W, H);
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].x0, 94.0);
        assert_eq!(regions[0].x1, 100.0);
        assert_eq!(regions[1].x0, 0.0);
        assert_eq!(regions[1].x1, 4.0);
    }

    #[test]
    fn corner_crossing_splits_in_four() {
        let regions = stamp_regions(stamp(1.0, 79.0, 4.0, 4.0), W, H);
        assert_eq!(regions.len(), 4);
        // Every piece is in-world and non-empty.
        for r in &regions {
            assert!(r.x0 < r.x1 && r.y0 < r.y1);
            assert!(r.x0 >= 0.0 && r.x1 <= W && r.y0 >= 0.0 && r.y1 <= H);
        }
    }

    #[test]
    fn oversized_stamp_clamps_to_world() {
        let regions = stamp_regions(stamp(50.0, 40.0, 300.0, 300.0), W, H);
        assert_eq!(
            regions,
            vec![RegionSpec {
                x0: 0.0,
                y0: 0.0,
                x1: W,
                y1: H
            }]
        );
    }

    #[test]
    fn out_of_world_cursor_normalizes() {
        // A cursor the camera reports outside [0, W) covers the same cells
        // as its wrapped twin.
        let a = stamp_regions(stamp(-98.0, 40.0, 5.0, 5.0), W, H);
        let b = stamp_regions(stamp(2.0, 40.0, 5.0, 5.0), W, H);
        assert_eq!(a, b);
    }

    #[test]
    fn pieces_tile_the_stamp_exactly() {
        // Sampled equivalence: a world point is inside exactly one emitted
        // piece iff it is inside the stamp under torus arithmetic — no gaps,
        // no double-covered overlap, for interior, seam, and corner stamps.
        let stamps = [
            stamp(50.0, 40.0, 10.0, 5.0),
            stamp(2.0, 40.0, 5.0, 5.0),
            stamp(99.5, 3.0, 6.0, 6.0),
            stamp(0.0, 0.0, 7.0, 7.0),
            stamp(50.0, 79.9, 10.0, 3.0),
        ];
        for s in stamps {
            let regions = stamp_regions(s, W, H);
            for i in 0..200 {
                for j in 0..160 {
                    let px = (i as f32 + 0.5) * 0.5;
                    let py = (j as f32 + 0.5) * 0.5;
                    let covered = regions.iter().filter(|r| r.contains(px, py)).count();
                    let expect = usize::from(in_stamp(px, py, s));
                    assert_eq!(
                        covered, expect,
                        "point ({px}, {py}) covered {covered}x under stamp {s:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn brush_drives_a_live_simulation_across_the_seam() {
        // End-to-end through the one scripted-action path: a seam-crossing
        // stamp queued into a running simulation edits exactly the wrapped
        // footprint — the UI tool is nothing but a script author.
        use genesis_config::{EnvFieldSpec, EnvSpec, FieldInit, SimConfig};
        use genesis_sim::Simulation;

        let config = SimConfig {
            particle_count: 0,
            world_width: 256.0,
            world_height: 256.0,
            env: EnvSpec {
                cols: 8,
                rows: 8,
                fields: vec![EnvFieldSpec {
                    name: String::new(),
                    init: FieldInit::Uniform(0.0),
                    dynamics: Default::default(),
                }],
            },
            ..SimConfig::default()
        };
        let mut sim = Simulation::new(&config);
        sim.tick();

        // Stamp centered on the west seam: half on each side. Cell width is
        // 32, so cells 0 and 7 (centers 16 and 240) are covered per row.
        let acts = stamp_actions(
            stamp(0.0, 128.0, 33.0, 300.0),
            0,
            FieldOp::Set(5.0),
            sim.tick_count(),
            256.0,
            256.0,
        );
        assert_eq!(acts.len(), 2, "seam stamp must be two records");
        for a in acts {
            sim.queue_action(a).unwrap();
        }
        sim.tick();

        let snap = sim.snapshot();
        for cy in 0..8usize {
            for cx in 0..8usize {
                let v = snap.env_fields[0][cy * 8 + cx];
                if cx == 0 || cx == 7 {
                    assert_eq!(v, 5.0, "seam cell ({cx}, {cy}) not painted");
                } else {
                    assert_eq!(v, 0.0, "interior cell ({cx}, {cy}) wrongly painted");
                }
            }
        }
    }

    #[test]
    fn actions_carry_op_field_and_tick_and_validate() {
        let acts = stamp_actions(stamp(99.0, 40.0, 5.0, 5.0), 2, FieldOp::Add(0.5), 42, W, H);
        assert_eq!(acts.len(), 2);
        for a in &acts {
            assert_eq!(a.tick, 42);
            match a.action {
                ActionKind::FieldAdd { field, delta, .. } => {
                    assert_eq!(field, 2);
                    assert_eq!(delta, 0.5);
                }
                _ => panic!("wrong kind"),
            }
        }
        // Emitted records pass the same validation a loaded script does.
        ActionScript {
            actions: stamp_actions(stamp(1.0, 79.0, 4.0, 4.0), 0, FieldOp::Set(1.0), 7, W, H),
        }
        .validate()
        .unwrap();
    }
}
