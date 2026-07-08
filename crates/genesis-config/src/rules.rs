//! Rule pack authoring schema (RON on disk).
//!
//! Rules are content, not code: declarative conditions, a fixed action
//! vocabulary, no scripting (decisions log, 2026-07-05). This module is the
//! *authoring* representation — friendly defaults, omittable fields. The
//! simulation compiles it into its internal `CompiledRule` form at load and
//! hashes the compiled pack into replay identity.
//!
//! Example pack:
//!
//! ```ron
//! (
//!     rules: [
//!         (
//!             radius: 6.0,
//!             self_cond: ( energy: ( min: 0.5 ) ),
//!             probability: 0.1,
//!             transfer: ( energy: 0.05 ),
//!         ),
//!     ],
//! )
//! ```

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ConfigError;

fn neg_inf() -> f32 {
    f32::NEG_INFINITY
}

fn inf() -> f32 {
    f32::INFINITY
}

/// Closed interval; omitted ends are unbounded.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BoundsSpec {
    pub min: f32,
    pub max: f32,
}

impl Default for BoundsSpec {
    fn default() -> Self {
        BoundsSpec {
            min: neg_inf(),
            max: inf(),
        }
    }
}

/// Conditions on one particle's quantities; omitted quantities match anything.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct ConditionSpec {
    pub matter: BoundsSpec,
    pub energy: BoundsSpec,
    pub information: BoundsSpec,
}

/// Amounts moved initiator → other when the rule fires; omitted = 0.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct TransferSpec {
    pub matter: f32,
    pub energy: f32,
    pub information: f32,
}

/// Bond created between initiator and other when the rule fires. `strength`
/// is the spring stiffness (see `PhysicsParams::bond_rest_length`). Creating
/// an already-existing bond is a no-op — bonds never stack.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct BondCreateSpec {
    pub strength: f32,
}

/// Lossy information copy: the initiator imprints its information value onto
/// the other particle (overwriting it), degraded by `noise`. The initiator
/// pays `cost` energy, which moves to the other particle so energy stays
/// conserved; if the initiator cannot pay the full cost the entire event
/// aborts (including its transfers). Information itself is NOT conserved —
/// copying creates it, decay (`physics.information_decay`) destroys it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct InfoCopySpec {
    /// Energy the initiator pays per copy (>= 0).
    pub cost: f32,
    /// Noise fraction in [0, 1]: the copied value is
    /// `src * (1 + noise * u)` with `u` uniform in [-1, 1], clamped >= 0.
    /// 0 = perfect copy.
    pub noise: f32,
}

/// Particle emission (split): the initiator spawns one new particle carrying
/// the given fractions of the initiator's current matter/energy/information —
/// moved, not copied, so every quantity is conserved by the event. The child
/// inherits the initiator's velocity exactly (momentum-exact) and appears
/// `offset` world units away at a deterministic per-pair angle. The event
/// aborts if the matter split would leave either side below the mass floor.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct EmitSpec {
    /// Fraction of the initiator's matter moved to the child (0..1).
    pub matter_frac: f32,
    /// Fraction of the initiator's energy moved to the child (0..1).
    pub energy_frac: f32,
    /// Fraction of the initiator's information moved to the child (0..1).
    pub info_frac: f32,
    /// Distance from the initiator where the child appears (> 0).
    pub offset: f32,
}

impl Default for EmitSpec {
    fn default() -> Self {
        EmitSpec {
            matter_frac: 0.5,
            energy_frac: 0.5,
            info_frac: 0.5,
            offset: 1.0,
        }
    }
}

/// Closed bounds on one environment field, sampled at the initiator's
/// position (Q-2026-07-08-A). `field` indexes the config's `env.fields` list;
/// omitted ends are unbounded.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct EnvBoundSpec {
    pub field: u32,
    #[serde(default = "neg_inf")]
    pub min: f32,
    #[serde(default = "inf")]
    pub max: f32,
}

/// One authored rule. `radius` and `probability` are mandatory; everything
/// else defaults to "match anything, move nothing".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuleSpec {
    pub radius: f32,
    #[serde(default)]
    pub self_cond: ConditionSpec,
    #[serde(default)]
    pub other_cond: ConditionSpec,
    /// Environment conditions: every listed field must be inside its bounds
    /// at the initiator's env cell for the rule to fire. Empty = fires
    /// anywhere. Field indices are checked against the config's declared
    /// fields at simulation assembly.
    #[serde(default)]
    pub env_cond: Vec<EnvBoundSpec>,
    pub probability: f32,
    #[serde(default)]
    pub transfer: TransferSpec,
    /// Create a bond between the pair (omitted = no bond action).
    #[serde(default)]
    pub bond_create: Option<BondCreateSpec>,
    /// Break the pair's bond if one exists. Mutually exclusive with
    /// `bond_create`.
    #[serde(default)]
    pub bond_break: bool,
    /// Copy the initiator's information onto the other particle (omitted =
    /// no copy).
    #[serde(default)]
    pub info_copy: Option<InfoCopySpec>,
    /// Spawn a new particle from the initiator's stocks (omitted = none).
    /// Mutually exclusive with `absorb`.
    #[serde(default)]
    pub emit: Option<EmitSpec>,
    /// The initiator absorbs the other particle: all of its quantities move
    /// to the initiator (velocity becomes the mass-weighted average) and the
    /// other is destroyed. Mutually exclusive with `emit`.
    #[serde(default)]
    pub absorb: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct RulePack {
    pub rules: Vec<RuleSpec>,
}

impl RulePack {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        // implicit_some lets packs write `bond_create: ( strength: k )`
        // instead of RON's literal `Some((...))`.
        let options = ron::Options::default()
            .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME);
        let pack: RulePack = options
            .from_str(&text)
            .map_err(|e| ConfigError::Parse(e.to_string()))?;
        pack.validate()?;
        Ok(pack)
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let pretty = ron::ser::PrettyConfig::default();
        let text = ron::ser::to_string_pretty(self, pretty)
            .map_err(|e| ConfigError::Parse(e.to_string()))?;
        std::fs::write(path, text)?;
        Ok(())
    }

    /// Structural validation. The radius-vs-grid-cell check happens at
    /// simulation assembly, where the physics parameters are known.
    pub fn validate(&self) -> Result<(), ConfigError> {
        for (i, rule) in self.rules.iter().enumerate() {
            let err = |msg: String| Err(ConfigError::Invalid(format!("rule {i}: {msg}")));
            if !(rule.radius > 0.0 && rule.radius.is_finite()) {
                return err(format!(
                    "radius must be positive and finite, got {}",
                    rule.radius
                ));
            }
            if !(0.0..=1.0).contains(&rule.probability) {
                return err(format!(
                    "probability must be in [0, 1], got {}",
                    rule.probability
                ));
            }
            for (name, v) in [
                ("matter", rule.transfer.matter),
                ("energy", rule.transfer.energy),
                ("information", rule.transfer.information),
            ] {
                if !(v >= 0.0 && v.is_finite()) {
                    return err(format!("transfer.{name} must be >= 0 and finite, got {v}"));
                }
            }
            if let Some(bond) = rule.bond_create {
                if rule.bond_break {
                    return err("bond_create and bond_break are mutually exclusive".into());
                }
                if !(bond.strength > 0.0 && bond.strength.is_finite()) {
                    return err(format!(
                        "bond_create.strength must be positive and finite, got {}",
                        bond.strength
                    ));
                }
            }
            if let Some(emit) = rule.emit {
                if rule.absorb {
                    return err("emit and absorb are mutually exclusive".into());
                }
                for (name, frac) in [
                    ("matter_frac", emit.matter_frac),
                    ("energy_frac", emit.energy_frac),
                    ("info_frac", emit.info_frac),
                ] {
                    if !(0.0..=1.0).contains(&frac) {
                        return err(format!("emit.{name} must be in [0, 1], got {frac}"));
                    }
                }
                if !(emit.offset > 0.0 && emit.offset.is_finite()) {
                    return err(format!(
                        "emit.offset must be positive and finite, got {}",
                        emit.offset
                    ));
                }
            }
            if let Some(copy) = rule.info_copy {
                if !(copy.cost >= 0.0 && copy.cost.is_finite()) {
                    return err(format!(
                        "info_copy.cost must be >= 0 and finite, got {}",
                        copy.cost
                    ));
                }
                if !(0.0..=1.0).contains(&copy.noise) {
                    return err(format!(
                        "info_copy.noise must be in [0, 1], got {}",
                        copy.noise
                    ));
                }
            }
            for (k, env) in rule.env_cond.iter().enumerate() {
                if env.min.is_nan() || env.max.is_nan() {
                    return err(format!("env_cond[{k}] bounds must not be NaN"));
                }
                if env.min > env.max {
                    return err(format!("env_cond[{k}] min {} > max {}", env.min, env.max));
                }
            }
            for (name, b) in [
                ("self_cond.matter", rule.self_cond.matter),
                ("self_cond.energy", rule.self_cond.energy),
                ("self_cond.information", rule.self_cond.information),
                ("other_cond.matter", rule.other_cond.matter),
                ("other_cond.energy", rule.other_cond.energy),
                ("other_cond.information", rule.other_cond.information),
            ] {
                if b.min.is_nan() || b.max.is_nan() {
                    return err(format!("{name} bounds must not be NaN"));
                }
                if b.min > b.max {
                    return err(format!("{name} min {} > max {}", b.min, b.max));
                }
            }
        }
        Ok(())
    }

    /// A small demonstration pack: energy diffuses from rich to poor
    /// particles, and heavy particles slowly accrete matter from light ones.
    pub fn example() -> Self {
        RulePack {
            rules: vec![
                RuleSpec {
                    radius: 6.0,
                    self_cond: ConditionSpec {
                        energy: BoundsSpec {
                            min: 0.5,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    other_cond: ConditionSpec {
                        energy: BoundsSpec {
                            max: 0.5,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    probability: 0.1,
                    transfer: TransferSpec {
                        energy: 0.05,
                        ..Default::default()
                    },
                    env_cond: Vec::new(),
                    bond_create: None,
                    bond_break: false,
                    info_copy: None,
                    emit: None,
                    absorb: false,
                },
                RuleSpec {
                    radius: 4.0,
                    self_cond: ConditionSpec {
                        matter: BoundsSpec {
                            max: 0.5,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    other_cond: ConditionSpec {
                        matter: BoundsSpec {
                            min: 0.5,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    probability: 0.02,
                    transfer: TransferSpec {
                        matter: 0.01,
                        ..Default::default()
                    },
                    env_cond: Vec::new(),
                    bond_create: None,
                    bond_break: false,
                    info_copy: None,
                    emit: None,
                    absorb: false,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_ron_fills_defaults() {
        let pack: RulePack = ron::from_str("(rules: [(radius: 5.0, probability: 0.5)])").unwrap();
        pack.validate().unwrap();
        let rule = &pack.rules[0];
        assert_eq!(rule.self_cond.matter.min, f32::NEG_INFINITY);
        assert_eq!(rule.self_cond.matter.max, f32::INFINITY);
        assert_eq!(rule.transfer.energy, 0.0);
    }

    #[test]
    fn bond_fields_parse_and_validate() {
        let dir = std::env::temp_dir().join(format!("genesis-bond-rules-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bonds.ron");
        std::fs::write(
            &path,
            "(rules: [
                (radius: 3.0, probability: 0.2, bond_create: (strength: 4.0)),
                (radius: 8.0, probability: 0.01, bond_break: true),
            ])",
        )
        .unwrap();
        let pack = RulePack::load(&path).unwrap();
        assert_eq!(pack.rules[0].bond_create.unwrap().strength, 4.0);
        assert!(pack.rules[1].bond_break);
        std::fs::remove_dir_all(&dir).ok();

        let mut bad = pack.clone();
        bad.rules[0].bond_break = true; // create + break on one rule
        assert!(bad.validate().is_err());

        let mut bad = pack.clone();
        bad.rules[0].bond_create = Some(BondCreateSpec { strength: 0.0 });
        assert!(bad.validate().is_err());
    }

    #[test]
    fn info_copy_fields_parse_and_validate() {
        let dir = std::env::temp_dir().join(format!("genesis-copy-rules-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("copy.ron");
        std::fs::write(
            &path,
            "(rules: [
                (radius: 6.0, probability: 0.1, info_copy: (cost: 0.05, noise: 0.2)),
            ])",
        )
        .unwrap();
        let pack = RulePack::load(&path).unwrap();
        assert_eq!(pack.rules[0].info_copy.unwrap().cost, 0.05);
        assert_eq!(pack.rules[0].info_copy.unwrap().noise, 0.2);
        std::fs::remove_dir_all(&dir).ok();

        let mut bad = pack.clone();
        bad.rules[0].info_copy = Some(InfoCopySpec {
            cost: -1.0,
            noise: 0.0,
        });
        assert!(bad.validate().is_err());

        let mut bad = pack.clone();
        bad.rules[0].info_copy = Some(InfoCopySpec {
            cost: 0.0,
            noise: 1.5,
        });
        assert!(bad.validate().is_err());
    }

    #[test]
    fn env_cond_parses_and_validates() {
        let pack: RulePack = ron::from_str(
            "(rules: [
                (radius: 5.0, probability: 0.5,
                 env_cond: [(field: 0, min: 0.4, max: 0.7), (field: 2, min: 1.0)]),
            ])",
        )
        .unwrap();
        pack.validate().unwrap();
        let env = &pack.rules[0].env_cond;
        assert_eq!(env.len(), 2);
        assert_eq!(env[0].field, 0);
        assert_eq!(env[0].min, 0.4);
        assert_eq!(env[0].max, 0.7);
        assert_eq!(env[1].field, 2);
        assert_eq!(env[1].max, f32::INFINITY, "omitted max is unbounded");

        // Omitted env_cond is empty (fires anywhere).
        let bare: RulePack = ron::from_str("(rules: [(radius: 5.0, probability: 0.5)])").unwrap();
        assert!(bare.rules[0].env_cond.is_empty());

        let mut bad = pack.clone();
        bad.rules[0].env_cond[0].min = 2.0; // min > max
        assert!(bad.validate().is_err());

        let mut bad = pack.clone();
        bad.rules[0].env_cond[0].max = f32::NAN;
        assert!(bad.validate().is_err());
    }

    #[test]
    fn emit_absorb_fields_parse_and_validate() {
        let dir = std::env::temp_dir().join(format!("genesis-ca-rules-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ca.ron");
        std::fs::write(
            &path,
            "(rules: [
                (radius: 6.0, probability: 0.05,
                 emit: (matter_frac: 0.5, energy_frac: 0.5, offset: 1.0)),
                (radius: 6.0, probability: 0.03, absorb: true),
            ])",
        )
        .unwrap();
        let pack = RulePack::load(&path).unwrap();
        assert_eq!(pack.rules[0].emit.unwrap().matter_frac, 0.5);
        assert_eq!(pack.rules[0].emit.unwrap().info_frac, 0.5, "default");
        assert!(pack.rules[1].absorb);
        std::fs::remove_dir_all(&dir).ok();

        let mut bad = pack.clone();
        bad.rules[0].absorb = true; // emit + absorb on one rule
        assert!(bad.validate().is_err());

        let mut bad = pack.clone();
        bad.rules[0].emit = Some(EmitSpec {
            matter_frac: 1.5,
            ..Default::default()
        });
        assert!(bad.validate().is_err());

        let mut bad = pack.clone();
        bad.rules[0].emit = Some(EmitSpec {
            offset: 0.0,
            ..Default::default()
        });
        assert!(bad.validate().is_err());
    }

    #[test]
    fn example_is_valid_and_roundtrips() {
        let pack = RulePack::example();
        pack.validate().unwrap();
        let text = ron::ser::to_string(&pack).unwrap();
        let back: RulePack = ron::from_str(&text).unwrap();
        assert_eq!(pack, back);
    }

    #[test]
    fn rejects_bad_rules() {
        let mut pack = RulePack::example();
        pack.rules[0].probability = 1.5;
        assert!(pack.validate().is_err());

        let mut pack = RulePack::example();
        pack.rules[0].radius = -1.0;
        assert!(pack.validate().is_err());

        let mut pack = RulePack::example();
        pack.rules[0].transfer.matter = f32::NAN;
        assert!(pack.validate().is_err());

        let mut pack = RulePack::example();
        pack.rules[0].self_cond.energy = BoundsSpec { min: 2.0, max: 1.0 };
        assert!(pack.validate().is_err());
    }

    #[test]
    fn all_shipped_packs_stay_valid() {
        // The packs/ directory is content the repo ships; schema changes
        // must never silently orphan them.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs");
        let mut checked = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "ron") {
                RulePack::load(&path)
                    .unwrap_or_else(|e| panic!("{} failed to load: {e}", path.display()));
                checked += 1;
            }
        }
        assert!(checked >= 6, "expected the shipped packs, found {checked}");
    }

    #[test]
    fn file_roundtrip() {
        let dir = std::env::temp_dir().join(format!("genesis-rules-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pack.ron");
        let pack = RulePack::example();
        pack.save(&path).unwrap();
        let back = RulePack::load(&path).unwrap();
        assert_eq!(pack, back);
        std::fs::remove_dir_all(&dir).ok();
    }
}
