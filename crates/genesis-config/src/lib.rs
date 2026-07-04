//! Simulation configuration: RON on disk, validated struct in memory.
//!
//! The configuration is part of replay identity (constitution rule 6):
//! same version + seed + config + player actions = same simulation.

use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

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
                speed: Range::new(0.0, 0.0),
            },
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
        if self.initial.matter.lo < 0.0 {
            return Err(ConfigError::Invalid("matter cannot be negative".into()));
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
