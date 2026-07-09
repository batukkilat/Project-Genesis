//! Extraction core: snapshot state → plain render data.
//!
//! Step 1 of docs/research/render-bootstrap.md (Q-2026-07-09-B). Everything
//! here is a pure function from a [`WorldSnapshot`] to a [`RenderFrame`] —
//! plain data with no references into the simulation. The windowed Bevy app
//! (step 2) consumes `RenderFrame` only, so promoting extraction onto a
//! dedicated simulation thread later changes no consumer code.
//!
//! Constitution: the renderer never owns simulation logic and never mutates
//! state — enforced here at the type level (`&WorldSnapshot` in, owned data
//! out). Visual mappings are data files a player can swap; they are never
//! replay identity (docs/design/visuals.md, principle 4).

pub mod pacer;
pub mod raster;

use std::path::Path;

use genesis_core::torus;
use genesis_sim::grid::GridGeom;
use genesis_sim::snapshot::WorldSnapshot;
use serde::{Deserialize, Serialize};

/// Zoom tiers from docs/design/visuals.md, selected by particles per screen
/// pixel — never draw millions of sprites (spec/Rendering.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Sprites + bond lines.
    T0Particle,
    /// Point cloud, no bonds.
    T1Cluster,
    /// Density/quantity heatmap from cell aggregates.
    T2Field,
    /// Whole-world heatmap (draw layer may mip further).
    T3Planet,
}

/// Camera: a torus-aware axis-aligned view. `width` is the horizontal world
/// extent on screen; height follows from the screen aspect ratio.
#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub center_x: f32,
    pub center_y: f32,
    pub width: f32,
}

/// One particle, mapped and placed in camera space (world units relative to
/// the camera center, torus-shortest). The draw layer only scales to pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpriteInstance {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    /// [0, 1] emissive brightness (energy).
    pub brightness: f32,
    /// [0, 1) position along the palette ramp (information).
    pub hue: f32,
}

/// One bond segment in camera space; alpha encodes strength.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BondLine {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub alpha: f32,
}

/// Per-cell aggregates over the sim's uniform grid (one linear pass; the
/// grid is pure geometry recomputed from snapshot params, so extraction
/// needs no access to simulation internals).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CellAgg {
    pub count: u32,
    pub mean_energy: f32,
    pub mean_information: f32,
}

/// Whole-world aggregate grid for the heatmap tiers.
#[derive(Debug, Clone, PartialEq)]
pub struct AggGrid {
    pub cols: u32,
    pub rows: u32,
    pub cell_w: f32,
    pub cell_h: f32,
    pub cells: Vec<CellAgg>,
}

/// Plain render data for one frame. Exactly one of `sprites`/`cells` is
/// populated, by tier; `bonds` only at T0.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderFrame {
    pub tier: Tier,
    pub sprites: Vec<SpriteInstance>,
    pub bonds: Vec<BondLine>,
    pub cells: Option<AggGrid>,
}

/// Quantity → visual mapping (docs/design/visuals.md v1 table). A RON data
/// file, hot-swappable, never replay identity.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct VisualMapping {
    /// Sprite radius = sqrt(matter) * radius_scale (mass reads as size).
    pub radius_scale: f32,
    /// Brightness = clamp(energy * brightness_scale) (hot things glow).
    pub brightness_scale: f32,
    /// Hue ramp position = fract(information * hue_scale).
    pub hue_scale: f32,
    /// Bond alpha = clamp(strength * bond_alpha_scale).
    pub bond_alpha_scale: f32,
}

impl Default for VisualMapping {
    fn default() -> Self {
        VisualMapping {
            radius_scale: 1.0,
            brightness_scale: 1.0,
            hue_scale: 1.0,
            bond_alpha_scale: 0.25,
        }
    }
}

impl VisualMapping {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let map: VisualMapping = ron::from_str(&text).map_err(|e| e.to_string())?;
        map.validate()?;
        Ok(map)
    }

    /// Mappings are player-swappable data: a bad file must fail at load,
    /// not poison `SpriteInstance`'s documented ranges downstream.
    pub fn validate(&self) -> Result<(), String> {
        for (name, v, signed) in [
            ("radius_scale", self.radius_scale, false),
            ("brightness_scale", self.brightness_scale, false),
            ("hue_scale", self.hue_scale, true),
            ("bond_alpha_scale", self.bond_alpha_scale, false),
        ] {
            if !v.is_finite() {
                return Err(format!("{name} must be finite, got {v}"));
            }
            if !signed && v < 0.0 {
                return Err(format!("{name} must be >= 0, got {v}"));
            }
        }
        Ok(())
    }
}

/// Wrap into [0, 1) with the same guard as `torus::wrap`: f32 `rem_euclid`
/// can round a tiny negative up to exactly the modulus.
fn wrap_unit(x: f32) -> f32 {
    let h = x.rem_euclid(1.0);
    if h >= 1.0 || h.is_nan() { 0.0 } else { h }
}

/// Pick the zoom tier from particles per screen pixel, assuming uniform
/// density (tier choice is presentation, not correctness — a wrong guess
/// draws a coarser or finer view, never a wrong one).
pub fn tier_for(snap: &WorldSnapshot, cam: &Camera, screen_w: u32, screen_h: u32) -> Tier {
    let view_w = cam.width.min(snap.world_width);
    let view_h = view_height(cam, screen_w, screen_h).min(snap.world_height);
    let density =
        snap.particles.len() as f32 / (snap.world_width as f64 * snap.world_height as f64) as f32;
    let visible = density * view_w * view_h;
    let pixels = (screen_w.max(1) as u64 * screen_h.max(1) as u64) as f32;
    let per_pixel = visible / pixels;
    if view_w >= snap.world_width && view_h >= snap.world_height {
        Tier::T3Planet
    } else if per_pixel < 1.0 {
        Tier::T0Particle
    } else if per_pixel < 100.0 {
        Tier::T1Cluster
    } else if per_pixel < 10_000.0 {
        Tier::T2Field
    } else {
        Tier::T3Planet
    }
}

fn view_height(cam: &Camera, screen_w: u32, screen_h: u32) -> f32 {
    cam.width * screen_h.max(1) as f32 / screen_w.max(1) as f32
}

/// Extract one frame. Pure and read-only: `&WorldSnapshot` in, plain data
/// out. Positions are camera-relative via the torus-shortest displacement,
/// so a view straddling the seam sees both sides at their correct relative
/// offsets with no special casing.
pub fn extract(
    snap: &WorldSnapshot,
    cam: &Camera,
    screen_w: u32,
    screen_h: u32,
    map: &VisualMapping,
) -> RenderFrame {
    let tier = tier_for(snap, cam, screen_w, screen_h);
    match tier {
        Tier::T0Particle | Tier::T1Cluster => {
            let sprites = extract_sprites(snap, cam, screen_w, screen_h, map);
            let bonds = if tier == Tier::T0Particle {
                extract_bonds(snap, cam, screen_w, screen_h, map)
            } else {
                Vec::new()
            };
            RenderFrame {
                tier,
                sprites,
                bonds,
                cells: None,
            }
        }
        Tier::T2Field | Tier::T3Planet => RenderFrame {
            tier,
            sprites: Vec::new(),
            bonds: Vec::new(),
            cells: Some(aggregate(snap)),
        },
    }
}

/// Integer multiples k of `period` for which the interval
/// `[lo + k·period, hi + k·period]` intersects `[-half, half]`. For a view
/// smaller than the world this is `0..=0` or empty; a view wider than the
/// world on an axis tiles wrapped copies across it (render-bootstrap.md:
/// duplicated instances at wrapped positions).
fn tile_range(lo: f32, hi: f32, period: f32, half: f32) -> std::ops::RangeInclusive<i32> {
    let k_lo = ((-half - hi) / period).ceil() as i32;
    let k_hi = ((half - lo) / period).floor() as i32;
    k_lo..=k_hi
}

fn extract_sprites(
    snap: &WorldSnapshot,
    cam: &Camera,
    screen_w: u32,
    screen_h: u32,
    map: &VisualMapping,
) -> Vec<SpriteInstance> {
    let half_w = 0.5 * cam.width;
    let half_h = 0.5 * view_height(cam, screen_w, screen_h);
    let cx = torus::wrap(cam.center_x, snap.world_width);
    let cy = torus::wrap(cam.center_y, snap.world_height);
    let mut out = Vec::new();
    for p in &snap.particles {
        let dx = torus::delta(cx, p.pos_x, snap.world_width);
        let dy = torus::delta(cy, p.pos_y, snap.world_height);
        let radius = p.matter.max(0.0).sqrt() * map.radius_scale;
        // Cull with a radius margin so a big sprite doesn't pop at the edge.
        for kx in tile_range(dx, dx, snap.world_width, half_w + radius) {
            for ky in tile_range(dy, dy, snap.world_height, half_h + radius) {
                out.push(SpriteInstance {
                    x: dx + kx as f32 * snap.world_width,
                    y: dy + ky as f32 * snap.world_height,
                    radius,
                    brightness: (p.energy * map.brightness_scale).clamp(0.0, 1.0),
                    hue: wrap_unit(p.information * map.hue_scale),
                });
            }
        }
    }
    out
}

fn extract_bonds(
    snap: &WorldSnapshot,
    cam: &Camera,
    screen_w: u32,
    screen_h: u32,
    map: &VisualMapping,
) -> Vec<BondLine> {
    if snap.bonds.is_empty() {
        return Vec::new();
    }
    let half_w = 0.5 * cam.width;
    let half_h = 0.5 * view_height(cam, screen_w, screen_h);
    let cx = torus::wrap(cam.center_x, snap.world_width);
    let cy = torus::wrap(cam.center_y, snap.world_height);
    // Particles are sorted by id (snapshot invariant): binary-search lookup.
    let pos_of = |id: u64| -> Option<(f32, f32)> {
        snap.particles
            .binary_search_by_key(&id, |p| p.id)
            .ok()
            .map(|i| (snap.particles[i].pos_x, snap.particles[i].pos_y))
    };
    let mut out = Vec::new();
    for b in &snap.bonds {
        let (Some((ax, ay)), Some((bx, by))) = (pos_of(b.a), pos_of(b.b)) else {
            continue;
        };
        let (dax, day) = (
            torus::delta(cx, ax, snap.world_width),
            torus::delta(cy, ay, snap.world_height),
        );
        // Draw the partner at its torus-shortest offset from endpoint A,
        // not from the camera — a bond crossing the seam then renders as
        // the short segment it physically is. Tiling below re-emits the
        // segment wherever a wrapped copy of either endpoint is visible,
        // so bond copies land exactly on sprite copies.
        let (dbx, dby) = (
            dax + torus::delta(ax, bx, snap.world_width),
            day + torus::delta(ay, by, snap.world_height),
        );
        let alpha = (b.strength * map.bond_alpha_scale).clamp(0.0, 1.0);
        // Cull by segment bounding box, not endpoint containment — a bond
        // longer than the view must still draw while crossing it.
        for kx in tile_range(dax.min(dbx), dax.max(dbx), snap.world_width, half_w) {
            for ky in tile_range(day.min(dby), day.max(dby), snap.world_height, half_h) {
                let (ox, oy) = (kx as f32 * snap.world_width, ky as f32 * snap.world_height);
                out.push(BondLine {
                    x0: dax + ox,
                    y0: day + oy,
                    x1: dbx + ox,
                    y1: dby + oy,
                    alpha,
                });
            }
        }
    }
    out
}

/// One linear pass over the snapshot onto the sim's uniform grid — the grid
/// is pure geometry (`GridGeom`), rebuilt from snapshot params.
fn aggregate(snap: &WorldSnapshot) -> AggGrid {
    let geom = GridGeom::new(snap.world_width, snap.world_height, snap.interaction_radius);
    let mut cells = vec![CellAgg::default(); geom.cell_count()];
    for p in &snap.particles {
        let c = geom.cell_of(p.pos_x, p.pos_y) as usize;
        let agg = &mut cells[c];
        agg.count += 1;
        agg.mean_energy += p.energy;
        agg.mean_information += p.information;
    }
    for agg in &mut cells {
        if agg.count > 0 {
            agg.mean_energy /= agg.count as f32;
            agg.mean_information /= agg.count as f32;
        }
    }
    AggGrid {
        cols: geom.cols,
        rows: geom.rows,
        cell_w: geom.cell_w,
        cell_h: geom.cell_h,
        cells,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use genesis_sim::snapshot::{BondSnap, ParticleSnap};

    /// Hand-built snapshot: only the fields extraction reads matter.
    fn snap(world: f32, particles: Vec<ParticleSnap>, bonds: Vec<BondSnap>) -> WorldSnapshot {
        WorldSnapshot {
            tick: 0,
            rng_state: 0,
            rng_gamma: 0,
            next_id: particles.len() as u64,
            stream_seed: 0,
            dt: 1.0 / 60.0,
            world_width: world,
            world_height: world,
            interaction_radius: 10.0,
            core_frac: 0.4,
            repulsion: 1.0,
            attraction: 0.0,
            bond_rest_length: 3.0,
            information_decay: 0.0,
            information_max: 1e30,
            lod: Default::default(),
            env_cols: 0,
            env_rows: 0,
            env_fields: Vec::new(),
            env_dynamics: Vec::new(),
            pending_actions: Vec::new(),
            rules: Vec::new(),
            particles,
            bonds,
        }
    }

    fn particle(id: u64, x: f32, y: f32, m: f32, e: f32, i: f32) -> ParticleSnap {
        ParticleSnap {
            id,
            pos_x: x,
            pos_y: y,
            vel_x: 0.0,
            vel_y: 0.0,
            matter: m,
            energy: e,
            information: i,
        }
    }

    #[test]
    fn tier_thresholds() {
        // 100 particles in a 100x100 world, 100x100 screen.
        let s = snap(
            100.0,
            (0..100)
                .map(|k| particle(k, 50.0, 50.0, 1.0, 0.0, 0.0))
                .collect(),
            vec![],
        );
        let cam = |w: f32| Camera {
            center_x: 50.0,
            center_y: 50.0,
            width: w,
        };
        // Tiny view: far under 1 per pixel.
        assert_eq!(tier_for(&s, &cam(10.0), 100, 100), Tier::T0Particle);
        // Whole world on screen = planet tier by definition.
        assert_eq!(tier_for(&s, &cam(100.0), 100, 100), Tier::T3Planet);
        // 10k particles (1/unit²), 10x10 screen, half the world visible:
        // 2.5k visible / 100 px = 25 per pixel -> T1.
        let mid = snap(
            100.0,
            (0..10_000)
                .map(|k| particle(k, 1.0, 1.0, 1.0, 0.0, 0.0))
                .collect(),
            vec![],
        );
        assert_eq!(tier_for(&mid, &cam(50.0), 10, 10), Tier::T1Cluster);
        // 100k particles (10/unit²): 25k visible / 100 px = 250 per pixel -> T2.
        let dense = snap(
            100.0,
            (0..100_000)
                .map(|k| particle(k, 1.0, 1.0, 1.0, 0.0, 0.0))
                .collect(),
            vec![],
        );
        assert_eq!(tier_for(&dense, &cam(50.0), 10, 10), Tier::T2Field);
    }

    #[test]
    fn sprites_are_camera_relative_across_the_seam() {
        // Camera on the seam of a 100-world: particles just left (x=98) and
        // right (x=3) of it must land at -2 and +3 in camera space.
        let s = snap(
            100.0,
            vec![
                particle(0, 98.0, 50.0, 1.0, 0.0, 0.0),
                particle(1, 3.0, 50.0, 1.0, 0.0, 0.0),
                particle(2, 50.0, 50.0, 1.0, 0.0, 0.0), // far side, not visible
            ],
            vec![],
        );
        let cam = Camera {
            center_x: 0.0,
            center_y: 50.0,
            width: 20.0,
        };
        let f = extract(&s, &cam, 100, 100, &VisualMapping::default());
        assert_eq!(f.tier, Tier::T0Particle);
        assert_eq!(f.sprites.len(), 2, "only the two seam-side particles");
        let xs: Vec<f32> = f.sprites.iter().map(|sp| sp.x).collect();
        assert!(xs.contains(&-2.0), "x=98 must appear at -2, got {xs:?}");
        assert!(xs.contains(&3.0), "x=3 must appear at +3, got {xs:?}");
    }

    #[test]
    fn mapping_drives_sprite_channels() {
        let s = snap(100.0, vec![particle(0, 50.0, 50.0, 4.0, 0.5, 0.25)], vec![]);
        let cam = Camera {
            center_x: 50.0,
            center_y: 50.0,
            width: 20.0,
        };
        let map = VisualMapping {
            radius_scale: 3.0,
            brightness_scale: 1.0,
            hue_scale: 2.0,
            bond_alpha_scale: 1.0,
        };
        let f = extract(&s, &cam, 100, 100, &map);
        let sp = f.sprites[0];
        assert_eq!(sp.radius, 6.0, "sqrt(4) * 3");
        assert_eq!(sp.brightness, 0.5);
        assert_eq!(sp.hue, 0.5, "fract(0.25 * 2)");
        // Brightness clamps.
        let hot = snap(100.0, vec![particle(0, 50.0, 50.0, 1.0, 9.0, 0.0)], vec![]);
        assert_eq!(
            extract(&hot, &cam, 100, 100, &map).sprites[0].brightness,
            1.0
        );
    }

    #[test]
    fn bonds_render_as_short_segments_across_the_seam() {
        // Bonded pair straddling the seam: endpoints at x=99 and x=1. The
        // line must span -1..+1 in camera space (length 2), never 98.
        let s = snap(
            100.0,
            vec![
                particle(0, 99.0, 50.0, 1.0, 0.0, 0.0),
                particle(1, 1.0, 50.0, 1.0, 0.0, 0.0),
            ],
            vec![BondSnap {
                a: 0,
                b: 1,
                strength: 2.0,
            }],
        );
        let cam = Camera {
            center_x: 0.0,
            center_y: 50.0,
            width: 20.0,
        };
        let f = extract(&s, &cam, 100, 100, &VisualMapping::default());
        assert_eq!(f.bonds.len(), 1);
        let b = f.bonds[0];
        assert!(
            (b.x1 - b.x0).abs() < 3.0,
            "seam bond must be the short segment, got {} .. {}",
            b.x0,
            b.x1
        );
        assert_eq!(b.alpha, 0.5, "strength 2 * default 0.25");
    }

    #[test]
    fn corner_camera_sees_all_four_quadrant_neighbors() {
        // Camera on the (0,0) corner of a 100-world: particles just inside
        // each of the four wrapped quadrants must all land near the origin
        // in camera space.
        let s = snap(
            100.0,
            vec![
                particle(0, 2.0, 3.0, 1.0, 0.0, 0.0),
                particle(1, 98.0, 3.0, 1.0, 0.0, 0.0),
                particle(2, 2.0, 97.0, 1.0, 0.0, 0.0),
                particle(3, 98.0, 97.0, 1.0, 0.0, 0.0),
            ],
            vec![],
        );
        let cam = Camera {
            center_x: 0.0,
            center_y: 0.0,
            width: 20.0,
        };
        let f = extract(&s, &cam, 100, 100, &VisualMapping::default());
        assert_eq!(f.sprites.len(), 4, "all four corner neighbors visible");
        let mut pts: Vec<(f32, f32)> = f.sprites.iter().map(|sp| (sp.x, sp.y)).collect();
        pts.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(
            pts,
            vec![(-2.0, -3.0), (-2.0, 3.0), (2.0, -3.0), (2.0, 3.0)]
        );
    }

    #[test]
    fn wider_than_world_view_tiles_wrapped_copies() {
        // Camera 300 wide over a 100-world: one particle must appear three
        // times, one world apart — never blank bands (render-bootstrap.md).
        let s = snap(100.0, vec![particle(0, 10.0, 50.0, 1.0, 0.0, 0.0)], vec![]);
        let cam = Camera {
            center_x: 50.0,
            center_y: 50.0,
            width: 300.0,
        };
        // 300x100 view on a 3:1 screen keeps view_h = 100 (T3 needs both
        // axes >= world; force sprites by keeping density tiny).
        let f = extract(&s, &cam, 300, 100, &VisualMapping::default());
        assert_eq!(f.tier, Tier::T3Planet, "whole world on screen is planet");
        // Drop to a sprite tier explicitly to test the tiling math.
        let sprites = extract_sprites(&s, &cam, 300, 100, &VisualMapping::default());
        let mut xs: Vec<f32> = sprites.iter().map(|sp| sp.x).collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(xs, vec![-140.0, -40.0, 60.0], "copies one world apart");
    }

    #[test]
    fn bond_longer_than_the_view_still_draws_while_crossing_it() {
        // Extreme zoom: view is 2 wide, bond endpoints at camera-space
        // -1.5 and +1.5 — both outside, segment spans the screen.
        let s = snap(
            100.0,
            vec![
                particle(0, 48.5, 50.0, 1.0, 0.0, 0.0),
                particle(1, 51.5, 50.0, 1.0, 0.0, 0.0),
            ],
            vec![BondSnap {
                a: 0,
                b: 1,
                strength: 1.0,
            }],
        );
        let cam = Camera {
            center_x: 50.0,
            center_y: 50.0,
            width: 2.0,
        };
        let bonds = extract_bonds(&s, &cam, 100, 100, &VisualMapping::default());
        assert_eq!(bonds.len(), 1, "crossing bond must not be culled");
        assert_eq!((bonds[0].x0, bonds[0].x1), (-1.5, 1.5));
    }

    #[test]
    fn near_world_view_bond_copies_land_on_sprite_copies() {
        // The wide-view consistency case: with tiling, every emitted bond
        // endpoint coincides with some emitted sprite position.
        let s = snap(
            100.0,
            vec![
                particle(0, 45.0, 50.0, 1.0, 0.0, 0.0),
                particle(1, 55.0, 50.0, 1.0, 0.0, 0.0),
            ],
            vec![BondSnap {
                a: 0,
                b: 1,
                strength: 1.0,
            }],
        );
        let cam = Camera {
            center_x: 0.0,
            center_y: 50.0,
            width: 96.0,
        };
        let map = VisualMapping::default();
        let sprites = extract_sprites(&s, &cam, 96, 20, &map);
        let bonds = extract_bonds(&s, &cam, 96, 20, &map);
        assert!(!bonds.is_empty());
        // Every bond endpoint that is on screen must have a sprite copy
        // under it (off-screen endpoints are rightly sprite-culled).
        let half_w = 48.0;
        let mut checked = 0;
        for b in &bonds {
            for ex in [b.x0, b.x1] {
                if ex.abs() <= half_w {
                    checked += 1;
                    assert!(
                        sprites.iter().any(|sp| (sp.x - ex).abs() < 1e-3),
                        "visible bond endpoint x={ex} has no sprite under it: {sprites:?}"
                    );
                }
            }
        }
        assert!(checked > 0, "test must actually check visible endpoints");
    }

    #[test]
    fn mapping_validation_rejects_bad_scales() {
        let mut m = VisualMapping::default();
        assert!(m.validate().is_ok());
        m.radius_scale = -1.0;
        assert!(m.validate().is_err(), "negative radius scale");
        m.radius_scale = 1.0;
        m.brightness_scale = f32::NAN;
        assert!(m.validate().is_err(), "NaN brightness scale");
        m.brightness_scale = 1.0;
        m.hue_scale = f32::INFINITY;
        assert!(m.validate().is_err(), "infinite hue scale");
        // Negative hue scale is a legal artistic choice (reversed ramp).
        m.hue_scale = -1.0;
        assert!(m.validate().is_ok());
    }

    #[test]
    fn hue_stays_in_unit_range_at_the_wrap_edge() {
        // f32 rem_euclid rounds tiny negatives to exactly 1.0 — the same
        // edge torus::wrap guards. Reversed ramps hit it via negative scale.
        assert_eq!(wrap_unit(-1e-8), 0.0);
        assert_eq!(wrap_unit(0.25), 0.25);
        assert_eq!(wrap_unit(f32::NAN), 0.0);
        let s = snap(100.0, vec![particle(0, 50.0, 50.0, 1.0, 0.0, 1e-8)], vec![]);
        let cam = Camera {
            center_x: 50.0,
            center_y: 50.0,
            width: 20.0,
        };
        let map = VisualMapping {
            hue_scale: -1.0,
            ..VisualMapping::default()
        };
        let f = extract(&s, &cam, 100, 100, &map);
        assert!(f.sprites[0].hue < 1.0, "hue must stay in [0, 1)");
    }

    #[test]
    fn aggregation_counts_and_means() {
        // 100-world, radius 10 -> 10x10 grid. Two particles in cell (0,0),
        // one in cell (9,9).
        let s = snap(
            100.0,
            vec![
                particle(0, 1.0, 1.0, 1.0, 2.0, 0.0),
                particle(1, 5.0, 5.0, 1.0, 4.0, 1.0),
                particle(2, 99.0, 99.0, 1.0, 8.0, 0.5),
            ],
            vec![],
        );
        let g = aggregate(&s);
        assert_eq!((g.cols, g.rows), (10, 10));
        let c00 = g.cells[0];
        assert_eq!(c00.count, 2);
        assert_eq!(c00.mean_energy, 3.0);
        assert_eq!(c00.mean_information, 0.5);
        let c99 = g.cells[99];
        assert_eq!(c99.count, 1);
        assert_eq!(c99.mean_energy, 8.0);
        // Everything lands somewhere; totals preserved.
        let total: u32 = g.cells.iter().map(|c| c.count).sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn mapping_loads_from_ron_with_defaults() {
        let m: VisualMapping = ron::from_str("(radius_scale: 2.0)").unwrap();
        assert_eq!(m.radius_scale, 2.0);
        assert_eq!(m.brightness_scale, 1.0, "unset fields take defaults");
        let full: VisualMapping = ron::from_str("()").unwrap();
        assert_eq!(full, VisualMapping::default());
    }

    #[test]
    fn empty_world_extracts_cleanly_at_every_tier() {
        let s = snap(100.0, vec![], vec![]);
        for w in [5.0, 50.0, 100.0] {
            let cam = Camera {
                center_x: 50.0,
                center_y: 50.0,
                width: w,
            };
            let f = extract(&s, &cam, 64, 64, &VisualMapping::default());
            assert!(f.sprites.is_empty());
            assert!(f.bonds.is_empty());
        }
    }
}
