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
        ron::from_str(&text).map_err(|e| e.to_string())
    }
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
    let pixels = (screen_w.max(1) * screen_h.max(1)) as f32;
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

fn extract_sprites(
    snap: &WorldSnapshot,
    cam: &Camera,
    screen_w: u32,
    screen_h: u32,
    map: &VisualMapping,
) -> Vec<SpriteInstance> {
    let half_w = 0.5 * cam.width.min(snap.world_width);
    let half_h = 0.5 * view_height(cam, screen_w, screen_h).min(snap.world_height);
    let cx = torus::wrap(cam.center_x, snap.world_width);
    let cy = torus::wrap(cam.center_y, snap.world_height);
    snap.particles
        .iter()
        .filter_map(|p| {
            let dx = torus::delta(cx, p.pos_x, snap.world_width);
            let dy = torus::delta(cy, p.pos_y, snap.world_height);
            if dx.abs() > half_w || dy.abs() > half_h {
                return None;
            }
            Some(SpriteInstance {
                x: dx,
                y: dy,
                radius: p.matter.max(0.0).sqrt() * map.radius_scale,
                brightness: (p.energy * map.brightness_scale).clamp(0.0, 1.0),
                hue: (p.information * map.hue_scale).rem_euclid(1.0),
            })
        })
        .collect()
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
    let half_w = 0.5 * cam.width.min(snap.world_width);
    let half_h = 0.5 * view_height(cam, screen_w, screen_h).min(snap.world_height);
    let cx = torus::wrap(cam.center_x, snap.world_width);
    let cy = torus::wrap(cam.center_y, snap.world_height);
    // Particles are sorted by id (snapshot invariant): binary-search lookup.
    let pos_of = |id: u64| -> Option<(f32, f32)> {
        snap.particles
            .binary_search_by_key(&id, |p| p.id)
            .ok()
            .map(|i| (snap.particles[i].pos_x, snap.particles[i].pos_y))
    };
    snap.bonds
        .iter()
        .filter_map(|b| {
            let (ax, ay) = pos_of(b.a)?;
            let (bx, by) = pos_of(b.b)?;
            let (dax, day) = (
                torus::delta(cx, ax, snap.world_width),
                torus::delta(cy, ay, snap.world_height),
            );
            let a_visible = dax.abs() <= half_w && day.abs() <= half_h;
            // Draw the partner at its torus-shortest offset from endpoint A,
            // not from the camera — a bond crossing the seam then renders as
            // the short segment it physically is.
            let (dbx, dby) = (
                dax + torus::delta(ax, bx, snap.world_width),
                day + torus::delta(ay, by, snap.world_height),
            );
            let b_visible = dbx.abs() <= half_w && dby.abs() <= half_h;
            if !a_visible && !b_visible {
                return None;
            }
            Some(BondLine {
                x0: dax,
                y0: day,
                x1: dbx,
                y1: dby,
                alpha: (b.strength * map.bond_alpha_scale).clamp(0.0, 1.0),
            })
        })
        .collect()
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
