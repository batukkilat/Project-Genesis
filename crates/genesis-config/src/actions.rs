//! Player action scripts (RON on disk) — Q-2026-07-08-B.
//!
//! A player action is a tick-stamped data record; a script is a list of
//! them. The Phase 6 UI will emit exactly these records interactively, so
//! scripted, recorded, and live play share one representation. Actions are
//! part of replay identity while pending (docs/research/player-actions.md):
//! same version + seed + config + actions = identical simulation
//! (constitution rule 6).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ConfigError;

/// Axis-aligned region in world coordinates: `x0 <= x < x1`, `y0 <= y < y1`,
/// clamped to the world. An env cell is affected iff its center falls inside.
/// A region wrapping the torus seam is authored as two rects.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct RegionSpec {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl RegionSpec {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x0 && x < self.x1 && y >= self.y0 && y < self.y1
    }

    fn validate(&self) -> Result<(), ConfigError> {
        for v in [self.x0, self.y0, self.x1, self.y1] {
            if !v.is_finite() {
                return Err(ConfigError::Invalid("region bounds must be finite".into()));
            }
        }
        if self.x0 > self.x1 || self.y0 > self.y1 {
            return Err(ConfigError::Invalid(format!(
                "region is inverted: ({}, {}) .. ({}, {})",
                self.x0, self.y0, self.x1, self.y1
            )));
        }
        Ok(())
    }
}

/// The player verbs. Environment-only, per the constitution (rule 4): the
/// vocabulary grows (rotation, tectonics, asteroid impacts) as the systems
/// they act on land — each as its own decisions-log amendment. Time warp is
/// deliberately absent: it cannot affect state and never enters replay
/// identity.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ActionKind {
    /// Set env field `field` to `value` in every cell whose center is inside
    /// `region`.
    FieldSet {
        field: u32,
        region: RegionSpec,
        value: f32,
    },
    /// Add `delta` to env field `field` in every cell whose center is inside
    /// `region`.
    FieldAdd {
        field: u32,
        region: RegionSpec,
        delta: f32,
    },
}

impl ActionKind {
    fn validate(&self) -> Result<(), ConfigError> {
        match self {
            ActionKind::FieldSet { region, value, .. } => {
                region.validate()?;
                if !value.is_finite() {
                    return Err(ConfigError::Invalid(
                        "field_set value must be finite".into(),
                    ));
                }
            }
            ActionKind::FieldAdd { region, delta, .. } => {
                region.validate()?;
                if !delta.is_finite() {
                    return Err(ConfigError::Invalid(
                        "field_add delta must be finite".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// One tick-stamped player action: applies at the very start of tick `tick`,
/// before anything else that tick simulates.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PlayerAction {
    pub tick: u64,
    pub action: ActionKind,
}

/// A headless play session: the full list of actions, applied in stamped-tick
/// order (stable within a tick — script order wins ties).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct ActionScript {
    pub actions: Vec<PlayerAction>,
}

impl ActionScript {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        let script: ActionScript =
            ron::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        script.validate()?;
        Ok(script)
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let pretty = ron::ser::PrettyConfig::default();
        let text = ron::ser::to_string_pretty(self, pretty)
            .map_err(|e| ConfigError::Parse(e.to_string()))?;
        std::fs::write(path, text)?;
        Ok(())
    }

    /// Structural validation. Field-index and tick-ordering checks against a
    /// live simulation happen at assembly, where field count and the current
    /// tick are known.
    pub fn validate(&self) -> Result<(), ConfigError> {
        for (i, a) in self.actions.iter().enumerate() {
            a.action
                .validate()
                .map_err(|e| ConfigError::Invalid(format!("action {i}: {e}")))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> RegionSpec {
        RegionSpec { x0, y0, x1, y1 }
    }

    #[test]
    fn script_parses_from_ron() {
        let script: ActionScript = ron::from_str(
            "(actions: [
                (tick: 100, action: FieldSet(field: 0, region: (x0: 0.0, y0: 0.0, x1: 64.0, y1: 128.0), value: 1.5)),
                (tick: 100, action: FieldAdd(field: 1, region: (x0: 32.0, y0: 0.0, x1: 96.0, y1: 64.0), delta: -0.5)),
            ])",
        )
        .unwrap();
        script.validate().unwrap();
        assert_eq!(script.actions.len(), 2);
        assert_eq!(script.actions[0].tick, 100);
        match script.actions[0].action {
            ActionKind::FieldSet { field, value, .. } => {
                assert_eq!(field, 0);
                assert_eq!(value, 1.5);
            }
            _ => panic!("wrong kind"),
        }
    }

    #[test]
    fn empty_script_is_valid() {
        let script: ActionScript = ron::from_str("()").unwrap();
        script.validate().unwrap();
        assert!(script.actions.is_empty());
    }

    #[test]
    fn region_contains_is_half_open() {
        let r = rect(0.0, 0.0, 10.0, 10.0);
        assert!(r.contains(0.0, 0.0));
        assert!(r.contains(9.999, 5.0));
        assert!(!r.contains(10.0, 5.0));
        assert!(!r.contains(-0.1, 5.0));
    }

    #[test]
    fn rejects_bad_actions() {
        let ok = PlayerAction {
            tick: 0,
            action: ActionKind::FieldSet {
                field: 0,
                region: rect(0.0, 0.0, 10.0, 10.0),
                value: 1.0,
            },
        };

        // Inverted region.
        let mut script = ActionScript { actions: vec![ok] };
        script.actions[0].action = ActionKind::FieldSet {
            field: 0,
            region: rect(10.0, 0.0, 0.0, 10.0),
            value: 1.0,
        };
        assert!(script.validate().is_err());

        // Non-finite value.
        let mut script = ActionScript { actions: vec![ok] };
        script.actions[0].action = ActionKind::FieldAdd {
            field: 0,
            region: rect(0.0, 0.0, 10.0, 10.0),
            delta: f32::NAN,
        };
        assert!(script.validate().is_err());

        // Non-finite region bound.
        let mut script = ActionScript { actions: vec![ok] };
        script.actions[0].action = ActionKind::FieldSet {
            field: 0,
            region: rect(0.0, 0.0, f32::INFINITY, 10.0),
            value: 1.0,
        };
        assert!(script.validate().is_err());

        // Degenerate (empty) region is a valid no-op, not an error.
        let mut script = ActionScript { actions: vec![ok] };
        script.actions[0].action = ActionKind::FieldSet {
            field: 0,
            region: rect(5.0, 5.0, 5.0, 5.0),
            value: 1.0,
        };
        script.validate().unwrap();
    }

    #[test]
    fn script_file_roundtrip() {
        let dir = std::env::temp_dir().join(format!("genesis-actions-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("script.ron");
        let script = ActionScript {
            actions: vec![PlayerAction {
                tick: 42,
                action: ActionKind::FieldAdd {
                    field: 2,
                    region: rect(1.0, 2.0, 3.0, 4.0),
                    delta: 0.25,
                },
            }],
        };
        script.save(&path).unwrap();
        let back = ActionScript::load(&path).unwrap();
        assert_eq!(script, back);
        std::fs::remove_dir_all(&dir).ok();
    }
}
