//! Simulation configuration: RON on disk, validated struct in memory.
//!
//! The configuration is part of replay identity (constitution rule 6):
//! same version + seed + config + player actions = same simulation.

pub mod actions;
pub mod rules;

use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub use actions::{ActionKind, ActionScript, PlayerAction, RegionSpec};
pub use rules::{BoundsSpec, ConditionSpec, EnvBoundSpec, RulePack, RuleSpec, TransferSpec};

/// Inclusive-exclusive range `[lo, hi)` used for initial particle quantities.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Range {
    pub lo: f32,
    pub hi: f32,
}

impl Range {
    pub const fn new(lo: f32, hi: f32) -> Self {
        Range { lo, hi }
    }
}

/// Initial ranges for the fundamental quantities of spawned particles.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InitialRanges {
    pub matter: Range,
    pub energy: Range,
    pub information: Range,
    /// Initial speed range; direction is uniform. Zero range = inert start.
    pub speed: Range,
}

/// Parameters of the generic short-range pairwise force kernel (Phase 2).
///
/// The kernel is Particle-Life-shaped: a linear repulsion core for
/// `r < core_frac * interaction_radius`, then a triangular attraction band
/// out to `interaction_radius`. Repulsion-only (attraction = 0) gives a gas;
/// adding attraction gives clustering, droplets, lattices — all from config.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PhysicsParams {
    /// Force cutoff distance; also the spatial grid cell size.
    pub interaction_radius: f32,
    /// Fraction of `interaction_radius` occupied by the repulsion core (0..1).
    pub core_frac: f32,
    /// Peak repulsion force magnitude (at r = 0).
    pub repulsion: f32,
    /// Peak attraction force magnitude (at the middle of the attraction band).
    pub attraction: f32,
    /// Rest length of bond springs. A bond pulls (or pushes) its endpoints
    /// toward this separation with force `strength * (r - rest_length)`;
    /// the per-bond strength is the spring stiffness.
    pub bond_rest_length: f32,
    /// Exponential decay rate of the information quantity, per simulated
    /// second (0 = no decay). Information is deliberately not conserved:
    /// copy actions create it, this destroys it (decisions log, 2026-07-05).
    pub information_decay: f32,
    /// Upper clamp on a particle's information, applied at interaction commit
    /// (decisions log, Q-2026-07-06-B). Amplifying rules saturate here instead
    /// of running to f32 overflow / NaN. Default 1e30 — far above any
    /// meaningful signal, far below the f32 range where transfer arithmetic
    /// overflows. Matter and energy are conserved by construction and stay
    /// uncapped. Part of replay identity: a different cap is a different
    /// universe.
    pub information_max: f32,
}

impl Default for PhysicsParams {
    fn default() -> Self {
        PhysicsParams {
            interaction_radius: 8.0,
            core_frac: 0.4,
            repulsion: 40.0,
            attraction: 5.0,
            bond_rest_length: 3.0,
            information_decay: 0.0,
            information_max: 1e30,
        }
    }
}

/// One rung of the adaptive-detail activity→rate ladder.
///
/// A chunk runs at the `rate` of the hottest rung whose `min_activity` its
/// activity metric reaches (see [`LodPolicy::rate_for`]). Rungs are ordered by
/// `min_activity` ascending; because rates are strictly decreasing along that
/// order, a hotter chunk always earns a smaller rate. `rate` is the tick
/// stride: a particle in a rate-`k` chunk is active on ticks where
/// `tick % k == 0`, and frozen bit-for-bit otherwise.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct LodRung {
    /// Minimum activity metric (chunk max of a non-negative per-particle
    /// scalar) to qualify for this rung.
    pub min_activity: f32,
    /// Tick stride for chunks on this rung. `1` = active every tick (hot).
    pub rate: u32,
}

/// Adaptive simulation detail (LOD) policy — Phase 4 groundwork
/// (docs/research/adaptive-detail.md). Quiet chunks tick at a reduced rate;
/// the policy is pure configuration, so with a fixed policy the classification
/// is a function of `(state, tick)` alone — deterministic, thread-count
/// invariant, and bit-identical across save/resume.
///
/// The policy becomes part of replay identity when LOD is wired into the state
/// hash (adaptive-detail landing step 5); until then it is inert config with a
/// disabled default, changing nothing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LodPolicy {
    /// Master switch. When false, every particle is active every tick (no LOD)
    /// and the rest of the policy is unused.
    pub enabled: bool,
    /// Side length, in grid cells, of a square LOD chunk. Coarser chunks are
    /// cheaper to classify but blunt the LOD boundary.
    pub chunk_cells: u32,
    /// Activity→rate ladder, ordered by `min_activity` ascending. Rung 0 must
    /// cover activity 0 (`min_activity == 0`, the coldest, largest rate) and
    /// the hottest rung must be rate 1, so active regions stay exact.
    pub ladder: Vec<LodRung>,
}

impl Default for LodPolicy {
    fn default() -> Self {
        // Disabled, all-hot ladder: a no-op even if switched on. Existing
        // configs that omit `lod` deserialize to exactly this.
        LodPolicy {
            enabled: false,
            chunk_cells: 8,
            ladder: vec![LodRung {
                min_activity: 0.0,
                rate: 1,
            }],
        }
    }
}

impl LodPolicy {
    /// The coldest (largest) rate any chunk can run at. Rung 0 is coldest by
    /// construction, so no chunk is ever frozen longer than this many ticks.
    pub fn max_rate(&self) -> u32 {
        self.ladder.first().map(|r| r.rate).unwrap_or(1)
    }

    /// Tick stride for a chunk with the given activity metric: the `rate` of
    /// the hottest rung the metric reaches. Callers must pass a non-negative,
    /// finite metric (chunk max of speed², which the info clamp keeps NaN-free).
    pub fn rate_for(&self, activity: f32) -> u32 {
        let mut rate = self.max_rate();
        for rung in &self.ladder {
            if activity >= rung.min_activity {
                rate = rung.rate;
            } else {
                break;
            }
        }
        rate
    }

    fn validate(&self) -> Result<(), ConfigError> {
        // A disabled policy is inert; its fields are never read, so don't
        // constrain them (an off switch should never fail to load).
        if !self.enabled {
            return Ok(());
        }
        if self.chunk_cells == 0 {
            return Err(ConfigError::Invalid("lod.chunk_cells must be >= 1".into()));
        }
        if self.ladder.is_empty() {
            return Err(ConfigError::Invalid(
                "lod.ladder must have at least one rung".into(),
            ));
        }
        if self.ladder[0].min_activity != 0.0 {
            return Err(ConfigError::Invalid(
                "lod.ladder rung 0 must have min_activity 0 (covers quiet chunks)".into(),
            ));
        }
        let mut prev_activity = f32::NEG_INFINITY;
        let mut prev_rate = u32::MAX;
        for rung in &self.ladder {
            if !rung.min_activity.is_finite() || rung.min_activity < 0.0 {
                return Err(ConfigError::Invalid(
                    "lod rung min_activity must be finite and >= 0".into(),
                ));
            }
            if rung.min_activity <= prev_activity {
                return Err(ConfigError::Invalid(
                    "lod ladder min_activity must be strictly ascending".into(),
                ));
            }
            if rung.rate < 1 {
                return Err(ConfigError::Invalid("lod rung rate must be >= 1".into()));
            }
            if rung.rate >= prev_rate {
                return Err(ConfigError::Invalid(
                    "lod ladder rate must be strictly decreasing as activity rises (hotter = faster)"
                        .into(),
                ));
            }
            prev_activity = rung.min_activity;
            prev_rate = rung.rate;
        }
        // The hottest rung must run every tick, so genuinely active regions are
        // simulated exactly — LOD only ever approximates the quiet.
        if self.ladder.last().unwrap().rate != 1 {
            return Err(ConfigError::Invalid(
                "lod ladder's hottest rung must have rate 1".into(),
            ));
        }
        Ok(())
    }
}

/// Initial value of an environment field, evaluated at each env-cell center
/// when the world is created. Fully consumed at creation (like `initial`
/// particle ranges): not part of replay identity — the resulting cell values
/// are.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum FieldInit {
    /// The same value everywhere.
    Uniform(f32),
    /// Linear west→east ramp: `lo` at x = 0, `hi` at x = world_width. The
    /// torus seam is a hard step by design.
    GradientX { lo: f32, hi: f32 },
    /// Linear north→south ramp: `lo` at y = 0, `hi` at y = world_height.
    GradientY { lo: f32, hi: f32 },
}

impl FieldInit {
    /// Value at a normalized cell-center coordinate (u, v) in [0, 1)².
    pub fn value_at(&self, u: f32, v: f32) -> f32 {
        match *self {
            FieldInit::Uniform(x) => x,
            FieldInit::GradientX { lo, hi } => lo + (hi - lo) * u,
            FieldInit::GradientY { lo, hi } => lo + (hi - lo) * v,
        }
    }

    fn validate(&self, i: usize) -> Result<(), ConfigError> {
        let finite = |v: f32| v.is_finite();
        let ok = match *self {
            FieldInit::Uniform(x) => finite(x),
            FieldInit::GradientX { lo, hi } | FieldInit::GradientY { lo, hi } => {
                finite(lo) && finite(hi)
            }
        };
        if ok {
            Ok(())
        } else {
            Err(ConfigError::Invalid(format!(
                "env.fields[{i}] init values must be finite"
            )))
        }
    }
}

/// Per-field dynamics (Q-2026-07-08-C): generic continuous operators run on
/// the env grid each tick, after the player-action drain and before the
/// particle step. Both default to 0 — a static field, bit-identical to a
/// world without dynamics. Part of replay identity when any rate is non-zero.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct FieldDynamics {
    /// Diffusion rate per second, in env-cell units: explicit 4-neighbor
    /// torus Laplacian, `v += diffusion * dt * (sum(neighbors) - 4v)`.
    /// Conserves the field total. Stability: `diffusion * dt <= 0.25`.
    pub diffusion: f32,
    /// Relaxation rate per second toward `relax_to`:
    /// `v += relax_rate * dt * (relax_to - v)`. The "climate" the field
    /// returns to after edits and disturbances. `relax_rate * dt <= 1`.
    pub relax_rate: f32,
    /// Rest value the relax operator approaches. Unused when `relax_rate`
    /// is 0.
    pub relax_to: f32,
}

impl FieldDynamics {
    /// A field with no dynamics is skipped entirely by the env step, so its
    /// cells stay untouched bits.
    pub fn is_static(&self) -> bool {
        self.diffusion == 0.0 && self.relax_rate == 0.0
    }
}

/// One declared environment field. The engine knows fields only by index;
/// `name` is documentation for authors and (later) UI/Observer labeling —
/// never read by the simulation, never hashed, never saved. Two configs
/// differing only in names are the same universe (Q-2026-07-08-A).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnvFieldSpec {
    #[serde(default)]
    pub name: String,
    pub init: FieldInit,
    /// Continuous per-tick evolution of the field; omitted = static.
    #[serde(default)]
    pub dynamics: FieldDynamics,
}

/// Planet-scale environment fields (Q-2026-07-08-A): generic indexed scalar
/// fields sampled on their own coarse torus-aligned grid, deliberately
/// decoupled from the interaction grid and LOD chunks. Empty (the default)
/// means no environment — nothing is stored, hashed, or paid for.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct EnvSpec {
    /// Env grid columns; all fields share one grid.
    pub cols: u32,
    /// Env grid rows.
    pub rows: u32,
    pub fields: Vec<EnvFieldSpec>,
}

impl Default for EnvSpec {
    fn default() -> Self {
        EnvSpec {
            cols: 32,
            rows: 32,
            fields: Vec::new(),
        }
    }
}

impl EnvSpec {
    fn validate(&self) -> Result<(), ConfigError> {
        // No fields = no environment; the grid dims are never read, so don't
        // constrain them (same posture as a disabled LOD policy).
        if self.fields.is_empty() {
            return Ok(());
        }
        if self.cols == 0 || self.rows == 0 {
            return Err(ConfigError::Invalid(
                "env.cols and env.rows must be >= 1 when fields are declared".into(),
            ));
        }
        for (i, f) in self.fields.iter().enumerate() {
            f.init.validate(i)?;
            let d = &f.dynamics;
            if !(d.diffusion >= 0.0 && d.diffusion.is_finite()) {
                return Err(ConfigError::Invalid(format!(
                    "env.fields[{i}].dynamics.diffusion must be >= 0 and finite"
                )));
            }
            if !(d.relax_rate >= 0.0 && d.relax_rate.is_finite()) {
                return Err(ConfigError::Invalid(format!(
                    "env.fields[{i}].dynamics.relax_rate must be >= 0 and finite"
                )));
            }
            if !d.relax_to.is_finite() {
                return Err(ConfigError::Invalid(format!(
                    "env.fields[{i}].dynamics.relax_to must be finite"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SimConfig {
    /// Master seed. Every RNG stream in the simulation derives from it.
    pub seed: u64,
    pub particle_count: u64,
    pub world_width: f32,
    pub world_height: f32,
    /// Fixed timestep: simulation ticks per simulated second.
    pub ticks_per_second: u32,
    pub initial: InitialRanges,
    pub physics: PhysicsParams,
    /// Adaptive simulation detail. Disabled by default — omit it and the
    /// simulation runs every particle every tick, exactly as before.
    pub lod: LodPolicy,
    /// Environment fields. Empty by default — omit it and the world has no
    /// environment, exactly as before.
    pub env: EnvSpec,
}

impl Default for SimConfig {
    fn default() -> Self {
        SimConfig {
            seed: 0,
            particle_count: 10_000,
            world_width: 4096.0,
            world_height: 4096.0,
            ticks_per_second: 60,
            initial: InitialRanges {
                matter: Range::new(0.1, 1.0),
                energy: Range::new(0.0, 1.0),
                information: Range::new(0.0, 0.0),
                speed: Range::new(0.0, 2.0),
            },
            physics: PhysicsParams::default(),
            lod: LodPolicy::default(),
            env: EnvSpec::default(),
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(String),
    Invalid(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "config io error: {e}"),
            ConfigError::Parse(e) => write!(f, "config parse error: {e}"),
            ConfigError::Invalid(e) => write!(f, "invalid config: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e)
    }
}

impl SimConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        let config: SimConfig =
            ron::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let pretty = ron::ser::PrettyConfig::default();
        let text = ron::ser::to_string_pretty(self, pretty)
            .map_err(|e| ConfigError::Parse(e.to_string()))?;
        std::fs::write(path, text)?;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        fn finite_range(name: &str, r: Range) -> Result<(), ConfigError> {
            if !r.lo.is_finite() || !r.hi.is_finite() {
                return Err(ConfigError::Invalid(format!("{name} range must be finite")));
            }
            if r.lo > r.hi {
                return Err(ConfigError::Invalid(format!(
                    "{name} range lo ({}) > hi ({})",
                    r.lo, r.hi
                )));
            }
            Ok(())
        }

        if self.world_width <= 0.0 || !self.world_width.is_finite() {
            return Err(ConfigError::Invalid("world_width must be positive".into()));
        }
        if self.world_height <= 0.0 || !self.world_height.is_finite() {
            return Err(ConfigError::Invalid("world_height must be positive".into()));
        }
        if self.ticks_per_second == 0 {
            return Err(ConfigError::Invalid("ticks_per_second must be > 0".into()));
        }
        finite_range("matter", self.initial.matter)?;
        finite_range("energy", self.initial.energy)?;
        finite_range("information", self.initial.information)?;
        finite_range("speed", self.initial.speed)?;
        if self.initial.matter.lo <= 0.0 {
            return Err(ConfigError::Invalid(
                "matter must be positive (it is the inertial mass)".into(),
            ));
        }

        let p = &self.physics;
        if p.interaction_radius <= 0.0 || !p.interaction_radius.is_finite() {
            return Err(ConfigError::Invalid(
                "interaction_radius must be positive".into(),
            ));
        }
        if !(0.0..1.0).contains(&p.core_frac) || p.core_frac == 0.0 {
            return Err(ConfigError::Invalid("core_frac must be in (0, 1)".into()));
        }
        if p.repulsion < 0.0 || !p.repulsion.is_finite() {
            return Err(ConfigError::Invalid("repulsion must be >= 0".into()));
        }
        if p.attraction < 0.0 || !p.attraction.is_finite() {
            return Err(ConfigError::Invalid("attraction must be >= 0".into()));
        }
        if p.bond_rest_length < 0.0 || !p.bond_rest_length.is_finite() {
            return Err(ConfigError::Invalid(
                "bond_rest_length must be >= 0 and finite".into(),
            ));
        }
        if p.information_decay < 0.0 || !p.information_decay.is_finite() {
            return Err(ConfigError::Invalid(
                "information_decay must be >= 0 and finite".into(),
            ));
        }
        // The cap must be a finite positive value: an infinite cap would
        // reintroduce the NaN overflow it exists to prevent, and a
        // non-positive cap would erase all information every commit.
        if p.information_max <= 0.0 || !p.information_max.is_finite() {
            return Err(ConfigError::Invalid(
                "information_max must be > 0 and finite".into(),
            ));
        }
        // Per-tick decay factor must stay in [0, 1]: a rate above 1/dt would
        // flip information negative.
        if p.information_decay * self.dt() > 1.0 {
            return Err(ConfigError::Invalid(format!(
                "information_decay {} too fast for dt {} (rate * dt must be <= 1)",
                p.information_decay,
                self.dt()
            )));
        }
        // The 3x3 neighbor-cell sweep double-counts cells unless the grid is
        // at least 3 cells in each axis.
        if (self.world_width / p.interaction_radius).floor() < 3.0
            || (self.world_height / p.interaction_radius).floor() < 3.0
        {
            return Err(ConfigError::Invalid(
                "world must be at least 3 interaction radii in each axis".into(),
            ));
        }
        self.lod.validate()?;
        self.env.validate()?;
        // Dynamics stability bounds depend on dt, so they live here rather
        // than in EnvSpec::validate. Explicit-Euler diffusion on a 4-neighbor
        // stencil is stable for rate * dt <= 1/4; relax must not overshoot.
        for (i, f) in self.env.fields.iter().enumerate() {
            if f.dynamics.diffusion * self.dt() > 0.25 {
                return Err(ConfigError::Invalid(format!(
                    "env.fields[{i}].dynamics.diffusion {} too fast for dt {} \
                     (rate * dt must be <= 0.25 for stability)",
                    f.dynamics.diffusion,
                    self.dt()
                )));
            }
            if f.dynamics.relax_rate * self.dt() > 1.0 {
                return Err(ConfigError::Invalid(format!(
                    "env.fields[{i}].dynamics.relax_rate {} too fast for dt {} \
                     (rate * dt must be <= 1)",
                    f.dynamics.relax_rate,
                    self.dt()
                )));
            }
        }
        Ok(())
    }

    /// Simulated seconds advanced per tick.
    pub fn dt(&self) -> f32 {
        1.0 / self.ticks_per_second as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_valid() {
        SimConfig::default().validate().unwrap();
    }

    #[test]
    fn ron_roundtrip() {
        let config = SimConfig {
            seed: 123,
            particle_count: 5,
            ..Default::default()
        };
        let text = ron::ser::to_string(&config).unwrap();
        let back: SimConfig = ron::from_str(&text).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn partial_ron_uses_defaults() {
        let config: SimConfig = ron::from_str("(seed: 7)").unwrap();
        assert_eq!(config.seed, 7);
        assert_eq!(config.particle_count, SimConfig::default().particle_count);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)] // mutate-one-field-per-case reads best here
    fn rejects_bad_values() {
        let mut config = SimConfig::default();
        config.ticks_per_second = 0;
        assert!(config.validate().is_err());

        let mut config = SimConfig::default();
        config.world_width = -1.0;
        assert!(config.validate().is_err());

        let mut config = SimConfig::default();
        config.initial.energy = Range::new(2.0, 1.0);
        assert!(config.validate().is_err());

        let mut config = SimConfig::default();
        config.initial.matter = Range::new(0.0, 1.0);
        assert!(
            config.validate().is_err(),
            "zero matter = zero mass, must be rejected"
        );

        let mut config = SimConfig::default();
        config.physics.core_frac = 1.5;
        assert!(config.validate().is_err());

        let mut config = SimConfig::default();
        config.physics.information_decay = -0.1;
        assert!(config.validate().is_err());

        let mut config = SimConfig::default();
        config.physics.information_decay = 100.0; // rate * dt > 1
        assert!(
            config.validate().is_err(),
            "decay faster than 1/dt must be rejected"
        );

        let mut config = SimConfig::default();
        config.physics.information_max = 0.0;
        assert!(
            config.validate().is_err(),
            "non-positive information_max must be rejected"
        );

        let mut config = SimConfig::default();
        config.physics.information_max = f32::INFINITY;
        assert!(
            config.validate().is_err(),
            "infinite information_max must be rejected (would allow NaN)"
        );

        let mut config = SimConfig::default();
        config.physics.interaction_radius = config.world_width;
        assert!(
            config.validate().is_err(),
            "grid narrower than 3 cells must be rejected"
        );
    }

    #[test]
    fn lod_default_is_disabled_and_valid() {
        let lod = LodPolicy::default();
        assert!(!lod.enabled);
        lod.validate().unwrap();
        SimConfig::default().validate().unwrap();
    }

    #[test]
    fn lod_omitted_from_ron_deserializes_to_disabled() {
        // Existing configs predate the `lod` field; they must still load.
        let config: SimConfig = ron::from_str("(seed: 7)").unwrap();
        assert_eq!(config.lod, LodPolicy::default());
        assert!(!config.lod.enabled);
    }

    #[test]
    fn lod_ron_roundtrip() {
        let config = SimConfig {
            lod: LodPolicy {
                enabled: true,
                chunk_cells: 4,
                ladder: vec![
                    LodRung {
                        min_activity: 0.0,
                        rate: 8,
                    },
                    LodRung {
                        min_activity: 0.5,
                        rate: 4,
                    },
                    LodRung {
                        min_activity: 4.0,
                        rate: 1,
                    },
                ],
            },
            ..Default::default()
        };
        config.validate().unwrap();
        let text = ron::ser::to_string(&config).unwrap();
        let back: SimConfig = ron::from_str(&text).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn lod_rate_for_picks_hottest_qualifying_rung() {
        let lod = LodPolicy {
            enabled: true,
            chunk_cells: 4,
            ladder: vec![
                LodRung {
                    min_activity: 0.0,
                    rate: 8,
                },
                LodRung {
                    min_activity: 0.5,
                    rate: 4,
                },
                LodRung {
                    min_activity: 4.0,
                    rate: 1,
                },
            ],
        };
        assert_eq!(lod.max_rate(), 8);
        assert_eq!(lod.rate_for(0.0), 8, "quiet chunk runs coldest");
        assert_eq!(lod.rate_for(0.49), 8);
        assert_eq!(lod.rate_for(0.5), 4, "boundary is inclusive");
        assert_eq!(lod.rate_for(3.99), 4);
        assert_eq!(lod.rate_for(4.0), 1, "hot chunk runs every tick");
        assert_eq!(lod.rate_for(1e6), 1);
    }

    #[test]
    fn lod_single_rung_all_hot_is_valid_noop() {
        let lod = LodPolicy {
            enabled: true,
            chunk_cells: 8,
            ladder: vec![LodRung {
                min_activity: 0.0,
                rate: 1,
            }],
        };
        lod.validate().unwrap();
        assert_eq!(lod.rate_for(0.0), 1);
        assert_eq!(lod.rate_for(999.0), 1);
    }

    #[test]
    fn lod_rejects_malformed_ladders() {
        let base = LodPolicy {
            enabled: true,
            chunk_cells: 4,
            ladder: vec![LodRung {
                min_activity: 0.0,
                rate: 1,
            }],
        };

        // chunk_cells zero.
        let mut p = base.clone();
        p.chunk_cells = 0;
        assert!(p.validate().is_err());

        // empty ladder.
        let mut p = base.clone();
        p.ladder = vec![];
        assert!(p.validate().is_err());

        // rung 0 not at activity 0.
        let mut p = base.clone();
        p.ladder = vec![LodRung {
            min_activity: 1.0,
            rate: 1,
        }];
        assert!(p.validate().is_err(), "rung 0 must cover quiet chunks");

        // non-ascending activity.
        let mut p = base.clone();
        p.ladder = vec![
            LodRung {
                min_activity: 0.0,
                rate: 4,
            },
            LodRung {
                min_activity: 0.0,
                rate: 1,
            },
        ];
        assert!(p.validate().is_err());

        // non-decreasing rate (hotter rung not faster).
        let mut p = base.clone();
        p.ladder = vec![
            LodRung {
                min_activity: 0.0,
                rate: 1,
            },
            LodRung {
                min_activity: 1.0,
                rate: 4,
            },
        ];
        assert!(p.validate().is_err());

        // hottest rung not rate 1.
        let mut p = base.clone();
        p.ladder = vec![
            LodRung {
                min_activity: 0.0,
                rate: 8,
            },
            LodRung {
                min_activity: 1.0,
                rate: 2,
            },
        ];
        assert!(p.validate().is_err(), "active regions must stay exact");

        // NaN threshold.
        let mut p = base.clone();
        p.ladder = vec![LodRung {
            min_activity: f32::NAN,
            rate: 1,
        }];
        assert!(p.validate().is_err());
    }

    #[test]
    fn lod_disabled_ignores_malformed_fields() {
        // A disabled policy must load even with a nonsense ladder — the switch
        // is off, the fields are unread.
        let p = LodPolicy {
            enabled: false,
            chunk_cells: 0,
            ladder: vec![],
        };
        p.validate().unwrap();
    }

    #[test]
    fn env_omitted_from_ron_deserializes_to_empty() {
        // Existing configs predate the `env` field; they must still load.
        let config: SimConfig = ron::from_str("(seed: 7)").unwrap();
        assert_eq!(config.env, EnvSpec::default());
        assert!(config.env.fields.is_empty());
        config.validate().unwrap();
    }

    #[test]
    fn env_ron_roundtrip() {
        let config = SimConfig {
            env: EnvSpec {
                cols: 16,
                rows: 8,
                fields: vec![
                    EnvFieldSpec {
                        name: "warmth".into(),
                        init: FieldInit::Uniform(1.5),
                        dynamics: Default::default(),
                    },
                    EnvFieldSpec {
                        name: String::new(),
                        init: FieldInit::GradientX { lo: 0.0, hi: 4.0 },
                        dynamics: Default::default(),
                    },
                ],
            },
            ..Default::default()
        };
        config.validate().unwrap();
        let text = ron::ser::to_string(&config).unwrap();
        let back: SimConfig = ron::from_str(&text).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn field_init_value_at() {
        assert_eq!(FieldInit::Uniform(3.0).value_at(0.9, 0.1), 3.0);
        let gx = FieldInit::GradientX { lo: 0.0, hi: 10.0 };
        assert_eq!(gx.value_at(0.0, 0.5), 0.0);
        assert_eq!(gx.value_at(0.5, 0.9), 5.0);
        let gy = FieldInit::GradientY { lo: -1.0, hi: 1.0 };
        assert_eq!(gy.value_at(0.3, 0.5), 0.0);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)] // mutate-one-field-per-case reads best here
    fn env_rejects_bad_values() {
        let field = |init| EnvFieldSpec {
            name: String::new(),
            init,
            dynamics: Default::default(),
        };

        // Zero grid with fields declared.
        let mut config = SimConfig::default();
        config.env = EnvSpec {
            cols: 0,
            rows: 8,
            fields: vec![field(FieldInit::Uniform(1.0))],
        };
        assert!(config.validate().is_err());

        // Non-finite init value.
        let mut config = SimConfig::default();
        config.env.fields = vec![field(FieldInit::Uniform(f32::NAN))];
        assert!(config.validate().is_err());

        let mut config = SimConfig::default();
        config.env.fields = vec![field(FieldInit::GradientX {
            lo: 0.0,
            hi: f32::INFINITY,
        })];
        assert!(config.validate().is_err());

        // No fields = inert: even a nonsense grid must load (the off switch
        // never fails).
        let mut config = SimConfig::default();
        config.env = EnvSpec {
            cols: 0,
            rows: 0,
            fields: vec![],
        };
        config.validate().unwrap();
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)] // mutate-one-field-per-case reads best here
    fn field_dynamics_parse_and_validate() {
        // Omitted dynamics deserialize to static zeros.
        let config: SimConfig =
            ron::from_str("(env: (cols: 4, rows: 4, fields: [(init: Uniform(1.0))]))").unwrap();
        assert_eq!(config.env.fields[0].dynamics, FieldDynamics::default());
        assert!(config.env.fields[0].dynamics.is_static());
        config.validate().unwrap();

        // Authored dynamics parse and roundtrip.
        let config: SimConfig = ron::from_str(
            "(env: (cols: 4, rows: 4, fields: [
                (init: Uniform(1.0), dynamics: (diffusion: 0.5, relax_rate: 0.1, relax_to: 1.0)),
            ]))",
        )
        .unwrap();
        config.validate().unwrap();
        assert_eq!(config.env.fields[0].dynamics.diffusion, 0.5);
        let text = ron::ser::to_string(&config).unwrap();
        let back: SimConfig = ron::from_str(&text).unwrap();
        assert_eq!(config, back);

        let field = |dynamics| EnvFieldSpec {
            name: String::new(),
            init: FieldInit::Uniform(0.0),
            dynamics,
        };

        // Negative rate.
        let mut config = SimConfig::default();
        config.env.fields = vec![field(FieldDynamics {
            diffusion: -1.0,
            ..Default::default()
        })];
        assert!(config.validate().is_err());

        // Unstable diffusion: rate * dt > 0.25 at 60 tps.
        let mut config = SimConfig::default();
        config.env.fields = vec![field(FieldDynamics {
            diffusion: 60.0,
            ..Default::default()
        })];
        assert!(
            config.validate().is_err(),
            "diffusion faster than 0.25/dt must be rejected (explicit Euler blows up)"
        );

        // Overshooting relax: rate * dt > 1.
        let mut config = SimConfig::default();
        config.env.fields = vec![field(FieldDynamics {
            relax_rate: 100.0,
            relax_to: 1.0,
            ..Default::default()
        })];
        assert!(config.validate().is_err());

        // Non-finite rest value.
        let mut config = SimConfig::default();
        config.env.fields = vec![field(FieldDynamics {
            relax_rate: 0.1,
            relax_to: f32::NAN,
            ..Default::default()
        })];
        assert!(config.validate().is_err());
    }

    #[test]
    fn file_roundtrip() {
        let dir = std::env::temp_dir().join(format!("genesis-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sim.ron");
        let config = SimConfig {
            seed: 99,
            ..Default::default()
        };
        config.save(&path).unwrap();
        let back = SimConfig::load(&path).unwrap();
        assert_eq!(config, back);
        std::fs::remove_dir_all(&dir).ok();
    }
}
