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

use crate::{ConfigError, Range};

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

/// Particle payload delivered by an asteroid impact (decisions log,
/// 2026-07-06): external material specified as *quantity ranges* — a region
/// of quantity space, never a named substance. Names/labels live above the
/// engine, in the Observer/UI layers.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PayloadSpec {
    /// Number of particles delivered.
    pub count: u32,
    /// Quantity ranges each payload particle draws from, uniformly.
    pub matter: Range,
    pub energy: Range,
    pub information: Range,
    /// Ejection speed range; direction is radially outward from the impact
    /// point (uniform angle for a particle spawned exactly at the point).
    pub speed: Range,
    /// Payload particles spawn uniformly on a disc of this radius around the
    /// impact point (0 = all at the point).
    pub spread: f32,
}

/// The player verbs. Environment-only, per the constitution (rule 4): the
/// vocabulary grows (rotation, tectonics) as the systems they act on land —
/// each as its own decisions-log amendment. Time warp is deliberately
/// absent: it cannot affect state and never enters replay identity.
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
    /// Asteroid impact at `(x, y)` (Q-2026-07-09-A, shape settled
    /// 2026-07-06): a momentum + energy shock to existing particles within
    /// `radius` (torus metric, linear falloff), plus a particle `payload`.
    /// Matter/energy arrive from outside the world by design — the exact
    /// injection is the payload quantities plus `energy`.
    Impact {
        x: f32,
        y: f32,
        /// Shock radius; particles farther than this are untouched.
        radius: f32,
        /// Peak momentum impulse (at the impact point, falling linearly to 0
        /// at `radius`), applied radially outward: dv = impulse * falloff /
        /// matter.
        impulse: f32,
        /// Total energy deposited across in-radius particles, split
        /// proportionally to falloff weight. Nothing is deposited when no
        /// particle is in radius.
        energy: f32,
        payload: PayloadSpec,
    },
    /// Tectonic event (Q-2026-07-10-C): an [`Impact`]-shaped shock whose
    /// source is a world-coordinate *segment* instead of a point — the
    /// mechanically honest v1 of the constitutional "tectonic events" verb.
    /// Particles within `radius` of the segment (torus metric; the segment
    /// vector is taken exactly as authored, so a segment may cross the seam)
    /// are pushed perpendicularly away from it with linear falloff; the
    /// declared energy splits across struck particles by falloff weight; the
    /// payload spawns like a point impact at a uniformly drawn point of the
    /// segment (upwelling). A degenerate segment (both endpoints equal)
    /// behaves exactly like an impact.
    ///
    /// [`Impact`]: ActionKind::Impact
    Rift {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        /// Shock radius around the segment; farther particles are untouched.
        radius: f32,
        /// Peak momentum impulse (on the segment, falling linearly to 0 at
        /// `radius`), applied perpendicularly away from the segment:
        /// dv = impulse * falloff / matter.
        impulse: f32,
        /// Total energy deposited across struck particles, split
        /// proportionally to falloff weight; lost when nobody is struck.
        energy: f32,
        payload: PayloadSpec,
    },
    /// Set the world frame's angular velocity (the planet-rotation verb,
    /// Q-2026-07-10-B): from its stamped tick on, every active particle
    /// feels the Coriolis acceleration `2·spin·perp(v)`. `spin` is a physics
    /// param in replay identity (when non-zero); setting it mid-run is state
    /// like an applied field edit.
    SpinSet {
        /// New angular velocity, radians per simulated second, either sign;
        /// 0 stops the rotation.
        spin: f32,
    },
}

impl ActionKind {
    /// Structural validity of one action (finite values, ordered region,
    /// sane payload). Public because live play queues single actions into a
    /// running simulation (Q-2026-07-08-B: the UI is just another script
    /// author) and must reject bad ones the same way script loading does.
    pub fn validate(&self) -> Result<(), ConfigError> {
        match *self {
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
            ActionKind::Impact {
                x,
                y,
                radius,
                impulse,
                energy,
                payload,
            } => {
                for (name, v) in [("x", x), ("y", y)] {
                    if !v.is_finite() {
                        return Err(ConfigError::Invalid(format!(
                            "impact {name} must be finite"
                        )));
                    }
                }
                if !radius.is_finite() || radius <= 0.0 {
                    return Err(ConfigError::Invalid(
                        "impact radius must be finite and > 0".into(),
                    ));
                }
                for (name, v) in [("impulse", impulse), ("energy", energy)] {
                    if !v.is_finite() || v < 0.0 {
                        return Err(ConfigError::Invalid(format!(
                            "impact {name} must be finite and >= 0"
                        )));
                    }
                }
                payload.validate()?;
            }
            ActionKind::Rift {
                x0,
                y0,
                x1,
                y1,
                radius,
                impulse,
                energy,
                payload,
            } => {
                for (name, v) in [("x0", x0), ("y0", y0), ("x1", x1), ("y1", y1)] {
                    if !v.is_finite() {
                        return Err(ConfigError::Invalid(format!("rift {name} must be finite")));
                    }
                }
                if !radius.is_finite() || radius <= 0.0 {
                    return Err(ConfigError::Invalid(
                        "rift radius must be finite and > 0".into(),
                    ));
                }
                for (name, v) in [("impulse", impulse), ("energy", energy)] {
                    if !v.is_finite() || v < 0.0 {
                        return Err(ConfigError::Invalid(format!(
                            "rift {name} must be finite and >= 0"
                        )));
                    }
                }
                payload.validate()?;
            }
            ActionKind::SpinSet { spin } => {
                if !spin.is_finite() {
                    return Err(ConfigError::Invalid("spin must be finite".into()));
                }
            }
        }
        Ok(())
    }
}

impl PayloadSpec {
    fn validate(&self) -> Result<(), ConfigError> {
        let ranges = [
            ("matter", self.matter),
            ("energy", self.energy),
            ("information", self.information),
            ("speed", self.speed),
        ];
        for (name, r) in ranges {
            if !r.lo.is_finite() || !r.hi.is_finite() || r.lo > r.hi {
                return Err(ConfigError::Invalid(format!(
                    "payload {name} range must be finite and ordered: [{}, {})",
                    r.lo, r.hi
                )));
            }
            if r.lo < 0.0 {
                return Err(ConfigError::Invalid(format!(
                    "payload {name} range must be non-negative"
                )));
            }
        }
        // Physics divides by matter (dv = F/m), so payload particles need
        // strictly positive mass — same constraint spawn config enforces.
        if self.count > 0 && self.matter.lo <= 0.0 {
            return Err(ConfigError::Invalid(
                "payload matter range must be strictly positive".into(),
            ));
        }
        if !self.spread.is_finite() || self.spread < 0.0 {
            return Err(ConfigError::Invalid(
                "payload spread must be finite and >= 0".into(),
            ));
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

    fn impact(radius: f32, impulse: f32, energy: f32, matter_lo: f32) -> ActionKind {
        ActionKind::Impact {
            x: 10.0,
            y: 20.0,
            radius,
            impulse,
            energy,
            payload: PayloadSpec {
                count: 5,
                matter: Range::new(matter_lo, 1.0),
                energy: Range::new(0.0, 2.0),
                information: Range::new(0.0, 0.0),
                speed: Range::new(0.5, 1.5),
                spread: 2.0,
            },
        }
    }

    #[test]
    fn impact_parses_from_ron() {
        let script: ActionScript = ron::from_str(
            "(actions: [
                (tick: 500, action: Impact(
                    x: 128.0, y: 64.0, radius: 20.0, impulse: 5.0, energy: 100.0,
                    payload: (count: 50,
                        matter: (lo: 0.2, hi: 0.8), energy: (lo: 1.0, hi: 3.0),
                        information: (lo: 0.0, hi: 0.0), speed: (lo: 0.5, hi: 2.0),
                        spread: 5.0),
                )),
            ])",
        )
        .unwrap();
        script.validate().unwrap();
        match script.actions[0].action {
            ActionKind::Impact {
                radius, payload, ..
            } => {
                assert_eq!(radius, 20.0);
                assert_eq!(payload.count, 50);
            }
            _ => panic!("wrong kind"),
        }
    }

    #[test]
    fn rejects_bad_impacts() {
        let cases = [
            impact(0.0, 1.0, 1.0, 0.1),           // zero radius
            impact(10.0, -1.0, 1.0, 0.1),         // negative impulse
            impact(10.0, 1.0, f32::NAN, 0.1),     // non-finite energy
            impact(10.0, 1.0, 1.0, 0.0),          // zero-mass payload
            impact(f32::INFINITY, 1.0, 1.0, 0.1), // non-finite radius
        ];
        for (i, action) in cases.into_iter().enumerate() {
            let script = ActionScript {
                actions: vec![PlayerAction { tick: 0, action }],
            };
            assert!(script.validate().is_err(), "case {i} must be rejected");
        }
        // A zero-count payload with a zero matter range is fine — it spawns
        // nothing, so the mass constraint is vacuous.
        let mut ok = impact(10.0, 1.0, 1.0, 0.0);
        if let ActionKind::Impact { payload, .. } = &mut ok {
            payload.count = 0;
        }
        ActionScript {
            actions: vec![PlayerAction {
                tick: 0,
                action: ok,
            }],
        }
        .validate()
        .unwrap();
    }

    #[test]
    fn rift_parses_and_rejects_like_an_impact() {
        let script: ActionScript = ron::from_str(
            "(actions: [
                (tick: 500, action: Rift(
                    x0: 32.0, y0: 64.0, x1: 96.0, y1: 64.0,
                    radius: 20.0, impulse: 5.0, energy: 100.0,
                    payload: (count: 50,
                        matter: (lo: 0.2, hi: 0.8), energy: (lo: 1.0, hi: 3.0),
                        information: (lo: 0.0, hi: 0.0), speed: (lo: 0.5, hi: 2.0),
                        spread: 5.0),
                )),
            ])",
        )
        .unwrap();
        script.validate().unwrap();

        // A degenerate (point) segment is valid; bad numbers are not.
        let ok = match script.actions[0].action {
            ActionKind::Rift { payload, .. } => payload,
            _ => panic!("wrong kind"),
        };
        let rift = |x1: f32, radius: f32, impulse: f32| ActionKind::Rift {
            x0: 32.0,
            y0: 64.0,
            x1,
            y1: 64.0,
            radius,
            impulse,
            energy: 1.0,
            payload: ok,
        };
        assert!(rift(32.0, 20.0, 5.0).validate().is_ok(), "point rift is ok");
        assert!(rift(f32::NAN, 20.0, 5.0).validate().is_err());
        assert!(rift(96.0, 0.0, 5.0).validate().is_err());
        assert!(rift(96.0, 20.0, -1.0).validate().is_err());
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
