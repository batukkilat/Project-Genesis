//! Versioned binary save/load for simulation snapshots.
//!
//! The format is hand-rolled rather than derived through a serialization
//! crate: save files are part of replay identity, and the byte layout must
//! never change because a dependency changed. Layout (all little-endian):
//!
//! ```text
//! magic            [u8; 4]  = b"GENS"
//! format_version   u32      = 12
//! engine_version   u16 len + utf-8 bytes (informational)
//! tick             u64
//! rng_state        u64
//! rng_gamma        u64
//! next_id          u64
//! stream_seed      u64      (v3: derived-stream base)
//! dt               f32
//! world_width      f32
//! world_height     f32
//! interaction_radius f32    (v2: physics params joined replay identity)
//! core_frac        f32
//! repulsion        f32
//! attraction       f32
//! bond_rest_length f32      (v4: bonds joined replay identity)
//! information_decay f32     (v5: information semantics joined replay identity)
//! information_max   f32      (v7: information overflow cap, Q-2026-07-06-B)
//! lod_enabled      u8       (v8: adaptive-detail policy; replay identity
//!                           only when enabled — see state_hash)
//! lod_chunk_cells  u32      (v8)
//! lod_ladder_len   u32      (v8)
//! lod_ladder       len * (min_activity f32, rate u32)   (v8)
//! env_cols         u32      (v9: environment fields joined replay identity —
//!                           only when declared; see state_hash)
//! env_rows         u32      (v9)
//! env_field_count  u32      (v9)
//! env_fields       count * (cols * rows f32 cell values, row-major)  (v9)
//! env_dynamics     count * (diffusion f32, relax_rate f32, relax_to f32)
//!                           (v12: field dynamics — replay identity only when
//!                           some field evolves; see state_hash)
//! pending_count    u32      (v11: pending player actions — replay identity
//!                           only when non-empty; see state_hash)
//! pending_actions  count * (tick u64, kind u8 (0 = set, 1 = add), field u32,
//!                           x0 f32, y0 f32, x1 f32, y1 f32, amount f32) (v11)
//! rule_count       u32      (v3: interaction rules joined replay identity)
//! rules            rule_count * (28 f32 core (CompiledRule::fields order; v4
//!                           appended bond action code + strength; v5
//!                           appended info-copy flag + cost + noise; v6
//!                           appended emit flag + fracs + offset + absorb flag),
//!                           then v10: env_cond_count u32 +
//!                           count * (field u32, min f32, max f32))
//! particle_count   u64
//! particles        count * (id u64, pos f32*2, vel f32*2, matter f32,
//!                           energy f32, information f32), sorted by id
//! bond_count       u64      (v4)
//! bonds            count * (a u64, b u64, strength f32), a < b, sorted (a, b)
//! state_hash       u64      (canonical hash of the snapshot, integrity check)
//! ```

use std::fmt;
use std::io::{Read, Write};
use std::path::Path;

use genesis_config::{ActionKind, FieldDynamics, LodPolicy, LodRung, PlayerAction, RegionSpec};
use genesis_sim::interact::{Bounds, CompiledRule, EnvBound};
use genesis_sim::snapshot::{BondSnap, ParticleSnap, WorldSnapshot};

pub const MAGIC: [u8; 4] = *b"GENS";
pub const FORMAT_VERSION: u32 = 12;

#[derive(Debug)]
pub enum SaveError {
    Io(std::io::Error),
    BadMagic,
    /// (found, supported)
    UnsupportedVersion(u32, u32),
    Corrupt(String),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::Io(e) => write!(f, "save io error: {e}"),
            SaveError::BadMagic => write!(f, "not a Genesis save file (bad magic)"),
            SaveError::UnsupportedVersion(found, supported) => {
                write!(
                    f,
                    "save format v{found} not supported (this build reads v{supported})"
                )
            }
            SaveError::Corrupt(msg) => write!(f, "corrupt save file: {msg}"),
        }
    }
}

impl std::error::Error for SaveError {}

impl From<std::io::Error> for SaveError {
    fn from(e: std::io::Error) -> Self {
        SaveError::Io(e)
    }
}

pub fn save_to_writer(snap: &WorldSnapshot, w: &mut impl Write) -> Result<(), SaveError> {
    w.write_all(&MAGIC)?;
    w.write_all(&FORMAT_VERSION.to_le_bytes())?;

    let engine_version = env!("CARGO_PKG_VERSION").as_bytes();
    w.write_all(&(engine_version.len() as u16).to_le_bytes())?;
    w.write_all(engine_version)?;

    w.write_all(&snap.tick.to_le_bytes())?;
    w.write_all(&snap.rng_state.to_le_bytes())?;
    w.write_all(&snap.rng_gamma.to_le_bytes())?;
    w.write_all(&snap.next_id.to_le_bytes())?;
    w.write_all(&snap.stream_seed.to_le_bytes())?;
    w.write_all(&snap.dt.to_le_bytes())?;
    w.write_all(&snap.world_width.to_le_bytes())?;
    w.write_all(&snap.world_height.to_le_bytes())?;
    w.write_all(&snap.interaction_radius.to_le_bytes())?;
    w.write_all(&snap.core_frac.to_le_bytes())?;
    w.write_all(&snap.repulsion.to_le_bytes())?;
    w.write_all(&snap.attraction.to_le_bytes())?;
    w.write_all(&snap.bond_rest_length.to_le_bytes())?;
    w.write_all(&snap.information_decay.to_le_bytes())?;
    w.write_all(&snap.information_max.to_le_bytes())?;

    // LOD policy (v8). Written unconditionally so the container stays self-
    // describing; it enters replay identity only when enabled.
    w.write_all(&[u8::from(snap.lod.enabled)])?;
    w.write_all(&snap.lod.chunk_cells.to_le_bytes())?;
    w.write_all(&(snap.lod.ladder.len() as u32).to_le_bytes())?;
    for rung in &snap.lod.ladder {
        w.write_all(&rung.min_activity.to_le_bytes())?;
        w.write_all(&rung.rate.to_le_bytes())?;
    }

    // Environment fields (v9). Written unconditionally so the container stays
    // self-describing; they enter replay identity only when declared.
    w.write_all(&snap.env_cols.to_le_bytes())?;
    w.write_all(&snap.env_rows.to_le_bytes())?;
    w.write_all(&(snap.env_fields.len() as u32).to_le_bytes())?;
    for field in &snap.env_fields {
        for v in field {
            w.write_all(&v.to_le_bytes())?;
        }
    }

    // Field dynamics (v12), one record per env field. Replay identity only
    // when some field evolves.
    for d in &snap.env_dynamics {
        w.write_all(&d.diffusion.to_le_bytes())?;
        w.write_all(&d.relax_rate.to_le_bytes())?;
        w.write_all(&d.relax_to.to_le_bytes())?;
    }

    // Pending player actions (v11). Written unconditionally (possibly count
    // 0); replay identity only when non-empty.
    w.write_all(&(snap.pending_actions.len() as u32).to_le_bytes())?;
    for a in &snap.pending_actions {
        w.write_all(&a.tick.to_le_bytes())?;
        let (code, field, region, amount) = match a.action {
            ActionKind::FieldSet {
                field,
                region,
                value,
            } => (0u8, field, region, value),
            ActionKind::FieldAdd {
                field,
                region,
                delta,
            } => (1u8, field, region, delta),
        };
        w.write_all(&[code])?;
        w.write_all(&field.to_le_bytes())?;
        w.write_all(&region.x0.to_le_bytes())?;
        w.write_all(&region.y0.to_le_bytes())?;
        w.write_all(&region.x1.to_le_bytes())?;
        w.write_all(&region.y1.to_le_bytes())?;
        w.write_all(&amount.to_le_bytes())?;
    }

    w.write_all(&(snap.rules.len() as u32).to_le_bytes())?;
    for rule in &snap.rules {
        for v in rule.fields() {
            w.write_all(&v.to_le_bytes())?;
        }
        // Env gate (v10): written unconditionally (possibly count 0) so the
        // container stays self-describing.
        w.write_all(&(rule.env_cond.len() as u32).to_le_bytes())?;
        for e in &rule.env_cond {
            w.write_all(&e.field.to_le_bytes())?;
            w.write_all(&e.bounds.min.to_le_bytes())?;
            w.write_all(&e.bounds.max.to_le_bytes())?;
        }
    }

    w.write_all(&(snap.particles.len() as u64).to_le_bytes())?;
    for p in &snap.particles {
        w.write_all(&p.id.to_le_bytes())?;
        w.write_all(&p.pos_x.to_le_bytes())?;
        w.write_all(&p.pos_y.to_le_bytes())?;
        w.write_all(&p.vel_x.to_le_bytes())?;
        w.write_all(&p.vel_y.to_le_bytes())?;
        w.write_all(&p.matter.to_le_bytes())?;
        w.write_all(&p.energy.to_le_bytes())?;
        w.write_all(&p.information.to_le_bytes())?;
    }

    w.write_all(&(snap.bonds.len() as u64).to_le_bytes())?;
    for b in &snap.bonds {
        w.write_all(&b.a.to_le_bytes())?;
        w.write_all(&b.b.to_le_bytes())?;
        w.write_all(&b.strength.to_le_bytes())?;
    }

    w.write_all(&snap.state_hash().to_le_bytes())?;
    Ok(())
}

pub fn load_from_reader(r: &mut impl Read) -> Result<WorldSnapshot, SaveError> {
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic)?;
    if magic != MAGIC {
        return Err(SaveError::BadMagic);
    }

    let version = read_u32(r)?;
    if version != FORMAT_VERSION {
        return Err(SaveError::UnsupportedVersion(version, FORMAT_VERSION));
    }

    let engine_version_len = read_u16(r)? as usize;
    let mut engine_version = vec![0u8; engine_version_len];
    r.read_exact(&mut engine_version)?;

    let tick = read_u64(r)?;
    let rng_state = read_u64(r)?;
    let rng_gamma = read_u64(r)?;
    let next_id = read_u64(r)?;
    let stream_seed = read_u64(r)?;
    let dt = read_f32(r)?;
    let world_width = read_f32(r)?;
    let world_height = read_f32(r)?;
    let interaction_radius = read_f32(r)?;
    let core_frac = read_f32(r)?;
    let repulsion = read_f32(r)?;
    let attraction = read_f32(r)?;
    let bond_rest_length = read_f32(r)?;
    let information_decay = read_f32(r)?;
    let information_max = read_f32(r)?;

    let lod_enabled = read_u8(r)? != 0;
    let lod_chunk_cells = read_u32(r)?;
    let lod_ladder_len = read_u32(r)?;
    let mut ladder = Vec::with_capacity(lod_ladder_len.min(1 << 16) as usize);
    for _ in 0..lod_ladder_len {
        let min_activity = read_f32(r)?;
        let rate = read_u32(r)?;
        ladder.push(LodRung { min_activity, rate });
    }
    let lod = LodPolicy {
        enabled: lod_enabled,
        chunk_cells: lod_chunk_cells,
        ladder,
    };

    let env_cols = read_u32(r)?;
    let env_rows = read_u32(r)?;
    let env_field_count = read_u32(r)?;
    let env_cell_count = env_cols as usize * env_rows as usize;
    let mut env_fields = Vec::with_capacity(env_field_count.min(1 << 10) as usize);
    for _ in 0..env_field_count {
        let mut field = Vec::with_capacity(env_cell_count.min(1 << 24));
        for _ in 0..env_cell_count {
            field.push(read_f32(r)?);
        }
        env_fields.push(field);
    }

    let mut env_dynamics = Vec::with_capacity(env_field_count.min(1 << 10) as usize);
    for _ in 0..env_field_count {
        env_dynamics.push(FieldDynamics {
            diffusion: read_f32(r)?,
            relax_rate: read_f32(r)?,
            relax_to: read_f32(r)?,
        });
    }

    let pending_count = read_u32(r)?;
    let mut pending_actions = Vec::with_capacity(pending_count.min(1 << 16) as usize);
    for _ in 0..pending_count {
        let tick = read_u64(r)?;
        let code = read_u8(r)?;
        let field = read_u32(r)?;
        let region = RegionSpec {
            x0: read_f32(r)?,
            y0: read_f32(r)?,
            x1: read_f32(r)?,
            y1: read_f32(r)?,
        };
        let amount = read_f32(r)?;
        // An unknown kind code decodes to a set (like the bond-action code,
        // the corrupt save then fails at its integrity-hash check).
        let action = if code == 1 {
            ActionKind::FieldAdd {
                field,
                region,
                delta: amount,
            }
        } else {
            ActionKind::FieldSet {
                field,
                region,
                value: amount,
            }
        };
        pending_actions.push(PlayerAction { tick, action });
    }

    let rule_count = read_u32(r)?;
    let mut rules = Vec::with_capacity(rule_count.min(1 << 20) as usize);
    for _ in 0..rule_count {
        let mut fields = [0.0f32; 28];
        for f in &mut fields {
            *f = read_f32(r)?;
        }
        let mut rule = CompiledRule::from_fields(fields);
        let env_cond_count = read_u32(r)?;
        rule.env_cond.reserve(env_cond_count.min(1 << 16) as usize);
        for _ in 0..env_cond_count {
            let field = read_u32(r)?;
            let min = read_f32(r)?;
            let max = read_f32(r)?;
            rule.env_cond.push(EnvBound {
                field,
                bounds: Bounds { min, max },
            });
        }
        rules.push(rule);
    }

    let count = read_u64(r)?;
    let mut particles = Vec::with_capacity(count.min(1 << 24) as usize);
    for _ in 0..count {
        particles.push(ParticleSnap {
            id: read_u64(r)?,
            pos_x: read_f32(r)?,
            pos_y: read_f32(r)?,
            vel_x: read_f32(r)?,
            vel_y: read_f32(r)?,
            matter: read_f32(r)?,
            energy: read_f32(r)?,
            information: read_f32(r)?,
        });
    }

    let bond_count = read_u64(r)?;
    let mut bonds = Vec::with_capacity(bond_count.min(1 << 24) as usize);
    for _ in 0..bond_count {
        bonds.push(BondSnap {
            a: read_u64(r)?,
            b: read_u64(r)?,
            strength: read_f32(r)?,
        });
    }

    let snap = WorldSnapshot {
        tick,
        rng_state,
        rng_gamma,
        next_id,
        stream_seed,
        dt,
        world_width,
        world_height,
        interaction_radius,
        core_frac,
        repulsion,
        attraction,
        bond_rest_length,
        information_decay,
        information_max,
        lod,
        env_cols,
        env_rows,
        env_fields,
        env_dynamics,
        pending_actions,
        rules,
        particles,
        bonds,
    };

    let stored_hash = read_u64(r)?;
    let actual_hash = snap.state_hash();
    if stored_hash != actual_hash {
        return Err(SaveError::Corrupt(format!(
            "state hash mismatch: stored {stored_hash:#018x}, computed {actual_hash:#018x}"
        )));
    }
    Ok(snap)
}

pub fn save_to_file(snap: &WorldSnapshot, path: &Path) -> Result<(), SaveError> {
    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    save_to_writer(snap, &mut file)?;
    file.flush()?;
    tracing::info!(path = %path.display(), tick = snap.tick, "saved simulation");
    Ok(())
}

pub fn load_from_file(path: &Path) -> Result<WorldSnapshot, SaveError> {
    let mut file = std::io::BufReader::new(std::fs::File::open(path)?);
    let snap = load_from_reader(&mut file)?;
    tracing::info!(path = %path.display(), tick = snap.tick, "loaded simulation");
    Ok(snap)
}

fn read_u8(r: &mut impl Read) -> Result<u8, SaveError> {
    let mut buf = [0u8; 1];
    r.read_exact(&mut buf)?;
    Ok(buf[0])
}

fn read_u16(r: &mut impl Read) -> Result<u16, SaveError> {
    let mut buf = [0u8; 2];
    r.read_exact(&mut buf)?;
    Ok(u16::from_le_bytes(buf))
}

fn read_u32(r: &mut impl Read) -> Result<u32, SaveError> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64(r: &mut impl Read) -> Result<u64, SaveError> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

fn read_f32(r: &mut impl Read) -> Result<f32, SaveError> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)?;
    Ok(f32::from_le_bytes(buf))
}

#[cfg(test)]
mod tests {
    use super::*;
    use genesis_config::SimConfig;
    use genesis_sim::Simulation;

    fn test_config() -> SimConfig {
        SimConfig {
            seed: 7,
            particle_count: 200,
            ..Default::default()
        }
    }

    #[test]
    fn roundtrip_in_memory() {
        let mut sim = Simulation::new(&test_config());
        for _ in 0..25 {
            sim.tick();
        }
        let snap = sim.snapshot();

        let mut bytes = Vec::new();
        save_to_writer(&snap, &mut bytes).unwrap();
        let back = load_from_reader(&mut bytes.as_slice()).unwrap();
        assert_eq!(snap, back);
        assert_eq!(snap.state_hash(), back.state_hash());
    }

    #[test]
    fn non_default_information_max_survives_the_format() {
        // Guards the v7 header field specifically: a non-default cap must
        // round-trip through the binary format and stay in replay identity
        // (the stored state hash covers it, so a dropped field would trip the
        // corruption check on load).
        let mut config = test_config();
        config.physics.information_max = 12.5;
        let mut sim = Simulation::new(&config);
        sim.tick();
        let snap = sim.snapshot();
        assert_eq!(snap.information_max, 12.5);

        let mut bytes = Vec::new();
        save_to_writer(&snap, &mut bytes).unwrap();
        let back = load_from_reader(&mut bytes.as_slice()).unwrap();
        assert_eq!(back.information_max, 12.5);
        assert_eq!(snap, back);
        assert_eq!(snap.state_hash(), back.state_hash());
    }

    #[test]
    fn enabled_lod_policy_survives_the_format() {
        // Guards the v8 header fields: an enabled policy (in replay identity)
        // must round-trip through the binary format bit-for-bit, and resuming
        // from it must reproduce the identical universe. A dropped or reordered
        // field would trip the stored state-hash check on load.
        use genesis_config::{LodPolicy, LodRung};
        let mut config = test_config();
        config.lod = LodPolicy {
            enabled: true,
            chunk_cells: 4,
            ladder: vec![
                LodRung {
                    min_activity: 0.0,
                    rate: 8,
                },
                LodRung {
                    min_activity: 1.5,
                    rate: 1,
                },
            ],
        };
        let mut sim = Simulation::new(&config);
        for _ in 0..20 {
            sim.tick();
        }
        let snap = sim.snapshot();
        assert!(snap.lod.enabled);

        let mut bytes = Vec::new();
        save_to_writer(&snap, &mut bytes).unwrap();
        let back = load_from_reader(&mut bytes.as_slice()).unwrap();
        assert_eq!(snap, back);
        assert_eq!(snap.state_hash(), back.state_hash());

        let mut resumed = Simulation::from_snapshot(&back);
        for _ in 0..40 {
            sim.tick();
            resumed.tick();
        }
        assert_eq!(sim.state_hash(), resumed.state_hash());
    }

    #[test]
    fn env_fields_survive_the_format() {
        // Guards the v9 and v12 blocks: declared fields and their dynamics
        // (in replay identity) must round-trip bit-for-bit, and resuming must
        // reproduce the identical universe — including future field evolution
        // driven by the saved dynamics params. A dropped or reordered value
        // would trip the stored state-hash check on load.
        use genesis_config::{EnvFieldSpec, EnvSpec, FieldDynamics, FieldInit};
        let mut config = test_config();
        config.env = EnvSpec {
            cols: 8,
            rows: 4,
            fields: vec![
                EnvFieldSpec {
                    name: "documentation only".into(),
                    init: FieldInit::GradientY { lo: -2.0, hi: 2.0 },
                    dynamics: FieldDynamics {
                        diffusion: 1.5,
                        relax_rate: 0.25,
                        relax_to: -1.0,
                    },
                },
                EnvFieldSpec {
                    name: String::new(),
                    init: FieldInit::Uniform(7.25),
                    dynamics: Default::default(),
                },
            ],
        };
        let mut sim = Simulation::new(&config);
        for _ in 0..20 {
            sim.tick();
        }
        let snap = sim.snapshot();
        assert_eq!(snap.env_fields.len(), 2);
        assert_eq!(snap.env_fields[0].len(), 32);

        let mut bytes = Vec::new();
        save_to_writer(&snap, &mut bytes).unwrap();
        let back = load_from_reader(&mut bytes.as_slice()).unwrap();
        assert_eq!(snap, back);
        assert_eq!(snap.state_hash(), back.state_hash());

        let mut resumed = Simulation::from_snapshot(&back);
        for _ in 0..40 {
            sim.tick();
            resumed.tick();
        }
        assert_eq!(sim.state_hash(), resumed.state_hash());
    }

    #[test]
    fn env_gated_rules_survive_the_format() {
        // Guards the v10 per-rule env block: a rule's env gate (in replay
        // identity) must round-trip bit-for-bit and resume into the identical
        // universe.
        use genesis_config::{EnvFieldSpec, EnvSpec, FieldInit};
        use genesis_sim::interact::{BondAction, EnvBound, QuantityCondition, RuleSet};
        let mut config = test_config();
        config.env = EnvSpec {
            cols: 8,
            rows: 8,
            fields: vec![EnvFieldSpec {
                name: String::new(),
                init: FieldInit::GradientX { lo: 0.0, hi: 1.0 },
                dynamics: Default::default(),
            }],
        };
        let rules = RuleSet {
            rules: vec![CompiledRule {
                radius: 4.0,
                env_cond: vec![EnvBound {
                    field: 0,
                    bounds: Bounds {
                        min: 0.5,
                        max: f32::INFINITY,
                    },
                }],
                self_cond: QuantityCondition::ANY,
                other_cond: QuantityCondition::ANY,
                probability: 0.3,
                transfer_matter: 0.0,
                transfer_energy: 0.01,
                transfer_information: 0.0,
                bond_action: BondAction::Create,
                bond_strength: 2.0,
                info_copy: false,
                info_cost: 0.0,
                info_noise: 0.0,
                emit: false,
                emit_matter_frac: 0.0,
                emit_energy_frac: 0.0,
                emit_info_frac: 0.0,
                emit_offset: 0.0,
                absorb: false,
            }],
        };
        let mut sim = Simulation::with_rules(&config, rules);
        for _ in 0..30 {
            sim.tick();
        }
        let snap = sim.snapshot();
        assert!(!snap.rules[0].env_cond.is_empty());

        let mut bytes = Vec::new();
        save_to_writer(&snap, &mut bytes).unwrap();
        let back = load_from_reader(&mut bytes.as_slice()).unwrap();
        assert_eq!(snap, back);
        assert_eq!(snap.state_hash(), back.state_hash());

        let mut resumed = Simulation::from_snapshot(&back);
        for _ in 0..40 {
            sim.tick();
            resumed.tick();
        }
        assert_eq!(sim.state_hash(), resumed.state_hash());
    }

    #[test]
    fn pending_actions_survive_the_format() {
        // Guards the v11 block: a mid-script save (one action applied, one
        // pending) must round-trip bit-for-bit and resume into the identical
        // future — the executable core of the Phase 4 exit criterion.
        use genesis_config::{
            ActionKind, ActionScript, EnvFieldSpec, EnvSpec, FieldInit, PlayerAction, RegionSpec,
        };
        use genesis_sim::interact::RuleSet;
        let mut config = test_config();
        config.env = EnvSpec {
            cols: 8,
            rows: 8,
            fields: vec![EnvFieldSpec {
                name: String::new(),
                init: FieldInit::Uniform(0.0),
                dynamics: Default::default(),
            }],
        };
        let act = |tick: u64, value: f32| PlayerAction {
            tick,
            action: ActionKind::FieldAdd {
                field: 0,
                region: RegionSpec {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 2048.0,
                    y1: 2048.0,
                },
                delta: value,
            },
        };
        let script = ActionScript {
            actions: vec![act(10, 1.0), act(50, -0.25)],
        };
        let make =
            || Simulation::with_rules_and_actions(&config, RuleSet::default(), script.clone());
        let mut sim = make();
        let mut uninterrupted = make();
        for _ in 0..30 {
            sim.tick();
            uninterrupted.tick();
        }
        let snap = sim.snapshot();
        assert_eq!(snap.pending_actions.len(), 1, "tick-50 action must pend");

        let mut bytes = Vec::new();
        save_to_writer(&snap, &mut bytes).unwrap();
        let back = load_from_reader(&mut bytes.as_slice()).unwrap();
        assert_eq!(snap, back);
        assert_eq!(snap.state_hash(), back.state_hash());

        let mut resumed = Simulation::from_snapshot(&back);
        for _ in 0..40 {
            uninterrupted.tick();
            resumed.tick();
        }
        assert_eq!(
            uninterrupted.state_hash(),
            resumed.state_hash(),
            "resume mid-script diverged (the pending tick-50 edit must fire \
             in both runs)"
        );
    }

    #[test]
    fn save_load_continue_matches_uninterrupted() {
        let config = test_config();
        let mut uninterrupted = Simulation::new(&config);
        let mut interrupted = Simulation::new(&config);

        for _ in 0..30 {
            uninterrupted.tick();
            interrupted.tick();
        }

        // Save, drop, reload.
        let mut bytes = Vec::new();
        save_to_writer(&interrupted.snapshot(), &mut bytes).unwrap();
        drop(interrupted);
        let mut resumed =
            Simulation::from_snapshot(&load_from_reader(&mut bytes.as_slice()).unwrap());

        for _ in 0..70 {
            uninterrupted.tick();
            resumed.tick();
        }
        assert_eq!(uninterrupted.state_hash(), resumed.state_hash());
        assert_eq!(resumed.tick_count(), 100);
    }

    #[test]
    fn roundtrip_preserves_bonds() {
        use genesis_sim::interact::{BondAction, QuantityCondition, RuleSet};
        let rules = RuleSet {
            rules: vec![CompiledRule {
                radius: 4.0,
                env_cond: Vec::new(),
                self_cond: QuantityCondition::ANY,
                other_cond: QuantityCondition::ANY,
                probability: 0.5,
                transfer_matter: 0.0,
                transfer_energy: 0.0,
                transfer_information: 0.0,
                bond_action: BondAction::Create,
                bond_strength: 2.0,
                info_copy: false,
                info_cost: 0.0,
                info_noise: 0.0,
                emit: false,
                emit_matter_frac: 0.0,
                emit_energy_frac: 0.0,
                emit_info_frac: 0.0,
                emit_offset: 0.0,
                absorb: false,
            }],
        };
        let config = SimConfig {
            seed: 7,
            particle_count: 200,
            world_width: 128.0,
            world_height: 128.0,
            ..Default::default()
        };
        let mut sim = Simulation::with_rules(&config, rules);
        for _ in 0..60 {
            sim.tick();
        }
        let snap = sim.snapshot();
        assert!(!snap.bonds.is_empty(), "test needs bonds in the save");

        let mut bytes = Vec::new();
        save_to_writer(&snap, &mut bytes).unwrap();
        let back = load_from_reader(&mut bytes.as_slice()).unwrap();
        assert_eq!(snap, back);

        // Resume from the loaded snapshot and stay hash-identical.
        let mut resumed = Simulation::from_snapshot(&back);
        for _ in 0..40 {
            sim.tick();
            resumed.tick();
        }
        assert_eq!(sim.state_hash(), resumed.state_hash());
    }

    #[test]
    fn rejects_bad_magic() {
        let bytes = b"NOPE, not a save file at all............";
        assert!(matches!(
            load_from_reader(&mut bytes.as_slice()),
            Err(SaveError::BadMagic)
        ));
    }

    #[test]
    fn rejects_unknown_version() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&999u32.to_le_bytes());
        assert!(matches!(
            load_from_reader(&mut bytes.as_slice()),
            Err(SaveError::UnsupportedVersion(999, FORMAT_VERSION))
        ));
    }

    #[test]
    fn detects_corruption() {
        let mut sim = Simulation::new(&test_config());
        sim.tick();
        let mut bytes = Vec::new();
        save_to_writer(&sim.snapshot(), &mut bytes).unwrap();
        // Flip one byte in the middle of the particle data.
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0xFF;
        assert!(matches!(
            load_from_reader(&mut bytes.as_slice()),
            Err(SaveError::Corrupt(_))
        ));
    }

    #[test]
    fn file_roundtrip() {
        let dir = std::env::temp_dir().join(format!("genesis-persist-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("world.gens");

        let mut sim = Simulation::new(&test_config());
        for _ in 0..5 {
            sim.tick();
        }
        let snap = sim.snapshot();
        save_to_file(&snap, &path).unwrap();
        let back = load_from_file(&path).unwrap();
        assert_eq!(snap, back);
        std::fs::remove_dir_all(&dir).ok();
    }
}
