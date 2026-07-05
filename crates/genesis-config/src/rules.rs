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

/// One authored rule. `radius` and `probability` are mandatory; everything
/// else defaults to "match anything, move nothing".
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct RuleSpec {
    pub radius: f32,
    #[serde(default)]
    pub self_cond: ConditionSpec,
    #[serde(default)]
    pub other_cond: ConditionSpec,
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
                    bond_create: None,
                    bond_break: false,
                    info_copy: None,
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
                    bond_create: None,
                    bond_break: false,
                    info_copy: None,
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
