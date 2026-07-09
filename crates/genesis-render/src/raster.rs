//! Heatmap rasterization: cell aggregates → low-res RGBA pixels.
//!
//! The logic half of landing step 3 (docs/research/render-bootstrap.md):
//! everything about the T2/T3 pixel look that needs no GPU. The Bevy layer
//! uploads the returned buffer as a texture onto the low-res offscreen
//! target and integer-upscales it; nothing here knows about windows.
//!
//! Style comes from the pipeline, not from art (docs/design/visuals.md):
//! a palette ramp quantized by ordered dithering gives the retro look.
//! Palettes are RON data files a player can swap — never replay identity.

use std::path::Path;

use genesis_core::torus;
use serde::{Deserialize, Serialize};

use crate::AggGrid;

/// Which aggregate quantity a heatmap view shows. A debug overlay is just
/// a different channel + palette; the machinery is identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeatChannel {
    /// Particle count per cell, normalized by `density_full` (reads as
    /// "where is matter").
    Density,
    /// Mean energy per cell, scaled by `energy_scale`.
    MeanEnergy,
    /// Mean information per cell, scaled by `information_scale`.
    MeanInformation,
}

/// A palette ramp: evenly spaced RGB stops, sampled by linear
/// interpolation over t ∈ [0, 1]. RON data, hot-swappable, never replay
/// identity (visuals.md principle 4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaletteRamp {
    pub stops: Vec<[u8; 3]>,
}

impl Default for PaletteRamp {
    /// Dark blue → cyan → yellow — a serviceable default until palette
    /// files ship with the app.
    fn default() -> Self {
        PaletteRamp {
            stops: vec![[8, 12, 40], [24, 96, 160], [72, 200, 208], [248, 232, 96]],
        }
    }
}

impl PaletteRamp {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let ramp: PaletteRamp = ron::from_str(&text).map_err(|e| e.to_string())?;
        ramp.validate()?;
        Ok(ramp)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.stops.is_empty() {
            return Err("palette ramp needs at least one stop".into());
        }
        Ok(())
    }

    /// Sample the ramp at t (clamped to [0, 1]).
    pub fn sample(&self, t: f32) -> [u8; 3] {
        let last = self.stops.len() - 1;
        if last == 0 {
            return self.stops[0];
        }
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
        let pos = t * last as f32;
        let i = (pos.floor() as usize).min(last - 1);
        let frac = pos - i as f32;
        let (a, b) = (self.stops[i], self.stops[i + 1]);
        [
            lerp_u8(a[0], b[0], frac),
            lerp_u8(a[1], b[1], frac),
            lerp_u8(a[2], b[2], frac),
        ]
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

/// Heatmap view parameters: channel, normalization, palette. Like
/// [`crate::VisualMapping`], plain data with serde defaults so partial RON
/// files work.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeatmapMapping {
    pub channel: HeatChannel,
    /// Cell count mapped to t = 1.0 for the Density channel.
    pub density_full: f32,
    /// Multiplier taking mean energy to t for MeanEnergy.
    pub energy_scale: f32,
    /// Multiplier taking mean information to t for MeanInformation.
    pub information_scale: f32,
    /// Dither amplitude in t units (0 disables; 1/16 quantizes to the
    /// Bayer grain). Applied before the palette lookup, so the ramp itself
    /// dithers — the retro look from visuals.md.
    pub dither: f32,
    pub palette: PaletteRamp,
}

impl Default for HeatmapMapping {
    fn default() -> Self {
        HeatmapMapping {
            channel: HeatChannel::Density,
            density_full: 8.0,
            energy_scale: 0.25,
            information_scale: 0.25,
            dither: 1.0 / 16.0,
            palette: PaletteRamp::default(),
        }
    }
}

impl HeatmapMapping {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let map: HeatmapMapping = ron::from_str(&text).map_err(|e| e.to_string())?;
        map.palette.validate()?;
        Ok(map)
    }

    /// Scalar t ∈ [0, 1] for one cell, before dithering.
    fn value(&self, agg: &crate::CellAgg) -> f32 {
        let raw = match self.channel {
            HeatChannel::Density => {
                if self.density_full > 0.0 {
                    agg.count as f32 / self.density_full
                } else {
                    0.0
                }
            }
            HeatChannel::MeanEnergy => agg.mean_energy * self.energy_scale,
            HeatChannel::MeanInformation => agg.mean_information * self.information_scale,
        };
        if raw.is_nan() {
            0.0
        } else {
            raw.clamp(0.0, 1.0)
        }
    }
}

/// 4×4 Bayer matrix, normalized to (-0.5, 0.5) threshold offsets.
const BAYER4: [[f32; 4]; 4] = [
    [0.0, 8.0, 2.0, 10.0],
    [12.0, 4.0, 14.0, 6.0],
    [3.0, 11.0, 1.0, 9.0],
    [15.0, 7.0, 13.0, 5.0],
];

fn bayer_offset(px: u32, py: u32) -> f32 {
    BAYER4[(py % 4) as usize][(px % 4) as usize] / 16.0 - 0.5
}

/// The world-rect a heatmap raster covers: origin (west/north edge, world
/// coordinates, may sit anywhere on the torus) plus extent. T3 passes the
/// whole world; T2 passes the camera rect.
#[derive(Debug, Clone, Copy)]
pub struct WorldRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Rasterize cell aggregates into a `target_w × target_h` RGBA8 buffer
/// (row-major, alpha 255). Each target pixel samples the aggregate cell
/// under its center (nearest-cell, torus-wrapped), takes the mapped scalar,
/// adds the Bayer threshold offset scaled by `dither`, and looks up the
/// palette. Pure and deterministic: same grid + rect + mapping ⇒ same bytes.
pub fn rasterize(
    grid: &AggGrid,
    rect: &WorldRect,
    target_w: u32,
    target_h: u32,
    map: &HeatmapMapping,
) -> Vec<u8> {
    let (tw, th) = (target_w.max(1), target_h.max(1));
    if grid.cells.is_empty() {
        // A degenerate grid rasters as the ramp floor rather than panicking;
        // the draw layer shows "nothing here", which is true.
        let [r, g, b] = map.palette.sample(0.0);
        return [r, g, b, 255].repeat((tw * th) as usize);
    }
    let world_w = grid.cell_w * grid.cols as f32;
    let world_h = grid.cell_h * grid.rows as f32;
    let mut out = Vec::with_capacity((tw * th * 4) as usize);
    for py in 0..th {
        // Pixel-center sampling keeps the raster stable under target-size
        // parity changes.
        let wy = torus::wrap(rect.y + rect.h * ((py as f32 + 0.5) / th as f32), world_h);
        let row = ((wy / grid.cell_h) as u32).min(grid.rows - 1);
        for px in 0..tw {
            let wx = torus::wrap(rect.x + rect.w * ((px as f32 + 0.5) / tw as f32), world_w);
            let col = ((wx / grid.cell_w) as u32).min(grid.cols - 1);
            let agg = &grid.cells[(row * grid.cols + col) as usize];
            let t = map.value(agg) + bayer_offset(px, py) * map.dither;
            let [r, g, b] = map.palette.sample(t);
            out.extend_from_slice(&[r, g, b, 255]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CellAgg;

    fn grid(cols: u32, rows: u32, cell: f32, cells: Vec<CellAgg>) -> AggGrid {
        assert_eq!(cells.len(), (cols * rows) as usize);
        AggGrid {
            cols,
            rows,
            cell_w: cell,
            cell_h: cell,
            cells,
        }
    }

    fn agg(count: u32, e: f32, i: f32) -> CellAgg {
        CellAgg {
            count,
            mean_energy: e,
            mean_information: i,
        }
    }

    #[test]
    fn palette_samples_endpoints_and_midpoints() {
        let p = PaletteRamp {
            stops: vec![[0, 0, 0], [100, 200, 50]],
        };
        assert_eq!(p.sample(0.0), [0, 0, 0]);
        assert_eq!(p.sample(1.0), [100, 200, 50]);
        assert_eq!(p.sample(0.5), [50, 100, 25]);
        // Out of range clamps; NaN maps to the floor.
        assert_eq!(p.sample(-1.0), [0, 0, 0]);
        assert_eq!(p.sample(2.0), [100, 200, 50]);
        assert_eq!(p.sample(f32::NAN), [0, 0, 0]);
        // Single-stop ramp is constant.
        let one = PaletteRamp {
            stops: vec![[7, 7, 7]],
        };
        assert_eq!(one.sample(0.0), [7, 7, 7]);
        assert_eq!(one.sample(1.0), [7, 7, 7]);
    }

    #[test]
    fn empty_palette_rejected_at_load_time() {
        let p = PaletteRamp { stops: vec![] };
        assert!(p.validate().is_err());
    }

    fn flat_map(channel: HeatChannel) -> HeatmapMapping {
        HeatmapMapping {
            channel,
            density_full: 4.0,
            energy_scale: 1.0,
            information_scale: 1.0,
            dither: 0.0, // exact-value tests want no dither
            palette: PaletteRamp {
                stops: vec![[0, 0, 0], [255, 255, 255]],
            },
        }
    }

    #[test]
    fn rasterize_maps_pixels_to_the_cells_under_them() {
        // 2x2 grid of 10-unit cells: counts 0, 4 top; 2, 0 bottom.
        let g = grid(
            2,
            2,
            10.0,
            vec![
                agg(0, 0.0, 0.0),
                agg(4, 0.0, 0.0),
                agg(2, 0.0, 0.0),
                agg(0, 0.0, 0.0),
            ],
        );
        let rect = WorldRect {
            x: 0.0,
            y: 0.0,
            w: 20.0,
            h: 20.0,
        };
        let px = rasterize(&g, &rect, 2, 2, &flat_map(HeatChannel::Density));
        // density_full = 4: count 4 -> white, count 2 -> mid gray, 0 -> black.
        assert_eq!(&px[0..4], &[0, 0, 0, 255], "top-left cell count 0");
        assert_eq!(&px[4..8], &[255, 255, 255, 255], "top-right cell count 4");
        assert_eq!(&px[8..12], &[128, 128, 128, 255], "bottom-left count 2");
        assert_eq!(&px[12..16], &[0, 0, 0, 255], "bottom-right count 0");
    }

    #[test]
    fn rasterize_wraps_the_rect_across_the_torus_seam() {
        // 2x1 grid, world 20x10. A rect starting at x=15 spans the seam:
        // its left half samples cell 1, its right half wraps to cell 0.
        let g = grid(2, 1, 10.0, vec![agg(4, 0.0, 0.0), agg(0, 0.0, 0.0)]);
        let rect = WorldRect {
            x: 15.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        let px = rasterize(&g, &rect, 2, 1, &flat_map(HeatChannel::Density));
        assert_eq!(&px[0..4], &[0, 0, 0, 255], "left pixel reads cell 1");
        assert_eq!(
            &px[4..8],
            &[255, 255, 255, 255],
            "right pixel wraps to cell 0"
        );
    }

    #[test]
    fn channels_select_their_quantity() {
        let g = grid(1, 1, 10.0, vec![agg(1, 0.75, 0.25)]);
        let rect = WorldRect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        let e = rasterize(&g, &rect, 1, 1, &flat_map(HeatChannel::MeanEnergy));
        assert_eq!(&e[0..3], &[191, 191, 191], "energy 0.75 on the gray ramp");
        let i = rasterize(&g, &rect, 1, 1, &flat_map(HeatChannel::MeanInformation));
        assert_eq!(&i[0..3], &[64, 64, 64], "information 0.25 on the gray ramp");
    }

    #[test]
    fn dither_perturbs_neighbors_but_stays_deterministic() {
        // A uniform mid-value field with dithering must not be uniform
        // (the Bayer offsets split it), and two runs must be identical.
        let g = grid(1, 1, 10.0, vec![agg(2, 0.0, 0.0)]);
        let rect = WorldRect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        let mut m = flat_map(HeatChannel::Density);
        m.dither = 0.5;
        let a = rasterize(&g, &rect, 4, 4, &m);
        let b = rasterize(&g, &rect, 4, 4, &m);
        assert_eq!(a, b, "same inputs, same bytes");
        let first = &a[0..4];
        assert!(
            a.chunks(4).any(|px| px != first),
            "dither must vary pixels of a uniform field"
        );
    }

    #[test]
    fn degenerate_grid_rasters_as_ramp_floor() {
        let g = AggGrid {
            cols: 0,
            rows: 0,
            cell_w: 1.0,
            cell_h: 1.0,
            cells: vec![],
        };
        let rect = WorldRect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        };
        let px = rasterize(&g, &rect, 2, 2, &flat_map(HeatChannel::Density));
        assert_eq!(px.len(), 16);
        assert_eq!(&px[0..4], &[0, 0, 0, 255]);
    }

    #[test]
    fn nan_aggregates_raster_as_floor_not_poison() {
        let g = grid(1, 1, 10.0, vec![agg(1, f32::NAN, f32::NAN)]);
        let rect = WorldRect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        let px = rasterize(&g, &rect, 1, 1, &flat_map(HeatChannel::MeanEnergy));
        assert_eq!(&px[0..4], &[0, 0, 0, 255]);
    }

    #[test]
    fn mapping_loads_from_ron_with_defaults() {
        let m: HeatmapMapping = ron::from_str("(channel: MeanEnergy)").unwrap();
        assert_eq!(m.channel, HeatChannel::MeanEnergy);
        assert_eq!(m.density_full, 8.0, "unset fields take defaults");
        let d: HeatmapMapping = ron::from_str("()").unwrap();
        assert_eq!(d, HeatmapMapping::default());
    }
}

#[cfg(test)]
mod palette_files {
    use super::PaletteRamp;
    use std::path::PathBuf;

    /// Every shipped palette file must load and validate — same contract
    /// the pack/config loaders enforce on their data directories.
    #[test]
    fn all_shipped_palettes_load() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../palettes");
        let mut seen = 0;
        for entry in std::fs::read_dir(&dir).expect("palettes/ must exist") {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "ron") {
                let ramp =
                    PaletteRamp::load(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                assert!(
                    ramp.stops.len() >= 2,
                    "{}: a ramp needs a gradient",
                    path.display()
                );
                seen += 1;
            }
        }
        assert!(seen >= 3, "expected the shipped palettes, found {seen}");
    }
}
