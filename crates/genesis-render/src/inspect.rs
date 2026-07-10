//! Inspector logic: cursor → particle → structure → camera focus.
//!
//! The logic half of landing step 5 (docs/research/render-bootstrap.md):
//! everything the observer panel and inspector need that is not Bevy
//! widgets. Pure read-only functions — the inspector observes, it never
//! touches simulation state, and annotations are panel-only (owner decision
//! 2026-07-08): selecting a structure may move the camera via
//! [`structure_focus`], but nothing is ever drawn on world objects.
//!
//! Panel *content* is the observer's own data ([`StructureMetrics`],
//! `Hypothesis`, `TimelineSample`) — this module only answers the geometric
//! questions in between: what is under the cursor, which structure owns it,
//! and where the camera should go to look at one.
//!
//! [`StructureMetrics`]: genesis_observer::StructureMetrics

use genesis_core::torus;
use genesis_observer::TrackedStructure;
use genesis_sim::snapshot::WorldSnapshot;

use crate::Camera;

impl Camera {
    /// World point under a screen position. Screen origin is top-left with
    /// +y down — the same orientation the raster tiers use (a rect origin is
    /// its west/north edge). `width` spans the viewport horizontally; height
    /// follows the viewport aspect. The result is wrapped into
    /// `[0, world) × [0, world)`.
    pub fn world_from_screen(
        &self,
        sx: f32,
        sy: f32,
        view_w: f32,
        view_h: f32,
        world_w: f32,
        world_h: f32,
    ) -> (f32, f32) {
        let height = self.width * view_h / view_w;
        let wx = self.center_x + (sx / view_w - 0.5) * self.width;
        let wy = self.center_y + (sy / view_h - 0.5) * height;
        (torus::wrap(wx, world_w), torus::wrap(wy, world_h))
    }
}

/// A picked particle: its id and its torus distance from the query point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PickHit {
    pub id: u64,
    pub dist: f32,
}

/// Nearest particle to a world point under the torus metric, within
/// `max_dist`. Deterministic: distance ties keep the lower id (snapshot
/// particles are sorted by id). A linear scan — inspector clicks are rare
/// and the whole-world extraction pass already set that budget precedent.
pub fn pick_particle(snap: &WorldSnapshot, x: f32, y: f32, max_dist: f32) -> Option<PickHit> {
    // Compare squared distances directly: re-squaring a stored sqrt is not
    // an identity in f32, and the round-trip error could hand a tie (or a
    // strictly farther particle in the error gap) to the higher id.
    let mut best: Option<(u64, f32)> = None;
    for p in &snap.particles {
        let dx = torus::delta(x, p.pos_x, snap.world_width);
        let dy = torus::delta(y, p.pos_y, snap.world_height);
        let d2 = dx * dx + dy * dy;
        if d2 <= max_dist * max_dist && best.is_none_or(|(_, b2)| d2 < b2) {
            best = Some((p.id, d2));
        }
    }
    best.map(|(id, d2)| PickHit {
        id,
        dist: d2.sqrt(),
    })
}

/// The tracked structure containing a particle, if any. Structures never
/// share members (they are connected components), so the first hit is the
/// only hit; member lists are sorted, so each check is a binary search.
pub fn structure_of(structures: &[TrackedStructure], particle: u64) -> Option<u64> {
    structures
        .iter()
        .find(|s| s.members.binary_search(&particle).is_ok())
        .map(|s| s.id)
}

/// Where the camera should center to look at a structure: the torus-aware
/// centroid of its members (per-axis circular mean, so a structure
/// straddling a seam resolves to the seam, never to the far side of the
/// world). Members no longer in the snapshot are skipped — the observer
/// samples less often than the sim ticks, so a member may have been
/// absorbed since the last sample. `None` when no member is present.
pub fn structure_focus(snap: &WorldSnapshot, members: &[u64]) -> Option<(f32, f32)> {
    let mut sum_x = (0.0f64, 0.0f64);
    let mut sum_y = (0.0f64, 0.0f64);
    let mut found = false;
    for &id in members {
        let Ok(i) = snap.particles.binary_search_by_key(&id, |p| p.id) else {
            continue;
        };
        found = true;
        let p = &snap.particles[i];
        let ax = f64::from(p.pos_x) / f64::from(snap.world_width) * std::f64::consts::TAU;
        let ay = f64::from(p.pos_y) / f64::from(snap.world_height) * std::f64::consts::TAU;
        sum_x = (sum_x.0 + ax.cos(), sum_x.1 + ax.sin());
        sum_y = (sum_y.0 + ay.cos(), sum_y.1 + ay.sin());
    }
    if !found {
        return None;
    }
    let unwrap = |(c, s): (f64, f64), size: f32| {
        let angle = s.atan2(c);
        torus::wrap((angle / std::f64::consts::TAU) as f32 * size, size)
    };
    Some((
        unwrap(sum_x, snap.world_width),
        unwrap(sum_y, snap.world_height),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use genesis_sim::snapshot::ParticleSnap;

    /// Hand-built snapshot: only the fields the inspector reads matter.
    fn snap(world: f32, particles: Vec<ParticleSnap>) -> WorldSnapshot {
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
            bonds: Vec::new(),
        }
    }

    fn particle(id: u64, x: f32, y: f32) -> ParticleSnap {
        ParticleSnap {
            id,
            pos_x: x,
            pos_y: y,
            vel_x: 0.0,
            vel_y: 0.0,
            matter: 1.0,
            energy: 0.0,
            information: 0.0,
        }
    }

    fn tracked(id: u64, members: Vec<u64>) -> TrackedStructure {
        TrackedStructure {
            id,
            members,
            age: 1,
            stability: 1.0,
        }
    }

    #[test]
    fn screen_to_world_pans_zooms_and_wraps() {
        let cam = Camera {
            center_x: 50.0,
            center_y: 50.0,
            width: 20.0,
        };
        // Screen center is the camera center.
        assert_eq!(
            cam.world_from_screen(400.0, 300.0, 800.0, 600.0, 100.0, 100.0),
            (50.0, 50.0)
        );
        // Top-left corner: half the view width west, half the view height
        // north (height = 20 * 600/800 = 15).
        assert_eq!(
            cam.world_from_screen(0.0, 0.0, 800.0, 600.0, 100.0, 100.0),
            (40.0, 42.5)
        );
        // A camera near the seam wraps: west of x=2 is x=97.
        let seam = Camera {
            center_x: 2.0,
            center_y: 50.0,
            width: 20.0,
        };
        let (wx, _) = seam.world_from_screen(0.0, 300.0, 800.0, 600.0, 100.0, 100.0);
        assert_eq!(wx, 92.0);
    }

    #[test]
    fn picks_nearest_within_radius_across_the_seam() {
        let s = snap(
            100.0,
            vec![particle(1, 99.0, 50.0), particle(2, 10.0, 50.0)],
        );
        // Query at x=1: particle 1 is 2 away across the seam, particle 2 is
        // 9 away directly.
        let hit = pick_particle(&s, 1.0, 50.0, 5.0).unwrap();
        assert_eq!(hit.id, 1);
        assert!((hit.dist - 2.0).abs() < 1e-5);
        // Nothing within a tight radius.
        assert_eq!(pick_particle(&s, 5.0, 50.0, 1.0), None);
    }

    #[test]
    fn pick_ties_keep_the_lower_id() {
        let s = snap(
            100.0,
            vec![particle(3, 48.0, 50.0), particle(7, 52.0, 50.0)],
        );
        // Equidistant from x=50.
        assert_eq!(pick_particle(&s, 50.0, 50.0, 5.0).unwrap().id, 3);
    }

    #[test]
    fn structure_lookup_by_member() {
        let structures = vec![tracked(10, vec![1, 4, 9]), tracked(11, vec![2, 3])];
        assert_eq!(structure_of(&structures, 4), Some(10));
        assert_eq!(structure_of(&structures, 3), Some(11));
        assert_eq!(structure_of(&structures, 5), None);
    }

    #[test]
    fn focus_of_a_seam_straddling_structure_is_on_the_seam() {
        let s = snap(100.0, vec![particle(1, 98.0, 50.0), particle(2, 2.0, 50.0)]);
        let (fx, fy) = structure_focus(&s, &[1, 2]).unwrap();
        // The naive mean would be 50 — the far side of the world. The
        // circular mean lands on the seam.
        assert!(
            !(1.0..=99.0).contains(&fx),
            "focus must be at the seam, got {fx}"
        );
        assert!((fy - 50.0).abs() < 1e-3);
    }

    #[test]
    fn focus_skips_absorbed_members_and_handles_all_gone() {
        let s = snap(
            100.0,
            vec![particle(1, 10.0, 10.0), particle(3, 20.0, 10.0)],
        );
        // Member 2 no longer exists in the snapshot; focus averages 1 and 3.
        let (fx, fy) = structure_focus(&s, &[1, 2, 3]).unwrap();
        assert!((fx - 15.0).abs() < 1e-3, "got {fx}");
        assert!((fy - 10.0).abs() < 1e-3);
        // Every member gone: nothing to focus on.
        assert_eq!(structure_focus(&s, &[7, 8]), None);
    }
}
