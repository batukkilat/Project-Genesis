//! Versioned binary save/load for simulation snapshots.
//!
//! The format is hand-rolled rather than derived through a serialization
//! crate: save files are part of replay identity, and the byte layout must
//! never change because a dependency changed. Layout (all little-endian):
//!
//! ```text
//! magic            [u8; 4]  = b"GENS"
//! format_version   u32      = 3
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
//! rule_count       u32      (v3: interaction rules joined replay identity)
//! rules            rule_count * 17 f32 (CompiledRule::fields order)
//! particle_count   u64
//! particles        count * (id u64, pos f32*2, vel f32*2, matter f32,
//!                           energy f32, information f32), sorted by id
//! state_hash       u64      (canonical hash of the snapshot, integrity check)
//! ```

use std::fmt;
use std::io::{Read, Write};
use std::path::Path;

use genesis_sim::interact::CompiledRule;
use genesis_sim::snapshot::{ParticleSnap, WorldSnapshot};

pub const MAGIC: [u8; 4] = *b"GENS";
pub const FORMAT_VERSION: u32 = 3;

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

    w.write_all(&(snap.rules.len() as u32).to_le_bytes())?;
    for rule in &snap.rules {
        for v in rule.fields() {
            w.write_all(&v.to_le_bytes())?;
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

    let rule_count = read_u32(r)?;
    let mut rules = Vec::with_capacity(rule_count.min(1 << 20) as usize);
    for _ in 0..rule_count {
        let mut fields = [0.0f32; 17];
        for f in &mut fields {
            *f = read_f32(r)?;
        }
        rules.push(CompiledRule::from_fields(fields));
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
        rules,
        particles,
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
