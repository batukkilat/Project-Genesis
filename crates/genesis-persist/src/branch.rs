//! Timeline branch records — the logic core of the Phase 6 branching
//! deliverable (Q-2026-07-10-A).
//!
//! A branch is an ordinary save file plus this RON sidecar: who it was
//! forked from (parent save path, state hash, and tick) and the player
//! actions applied on this branch since. Ancestry lives deliberately
//! **above the engine**: the binary save format is untouched, the engine
//! never reads a record, and nothing here enters replay identity — the
//! same precedent as field names and observer bookkeeping. Forking copies
//! the save (the child inherits the exact state, pending actions
//! included) and starts an empty log; two children of one parent share
//! ancestry by referencing the same chain of records.
//!
//! Replay of a branch is its ancestry chain replayed root-first plus this
//! log — packaging that into a single shareable file is Phase 7's
//! deliverable and may change containers without touching identity.

use std::path::Path;

use genesis_config::PlayerAction;
use genesis_sim::snapshot::WorldSnapshot;
use serde::{Deserialize, Serialize};

use crate::SaveError;

/// Current branch-record format. Bumped only for incompatible layout
/// changes; unknown formats are refused at load, like the binary save.
pub const BRANCH_FORMAT: u32 = 1;

/// Where a branch came from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParentRef {
    /// The parent branch's save file, as given at fork time — recorded
    /// verbatim, not canonicalized, so records stay portable alongside
    /// their saves. Two forks must name the parent the same way to be
    /// textually recognizable as siblings; the robust chain identity is
    /// `state_hash`, and path resolution is the consumer's concern
    /// (Phase 7 packaging).
    pub save: String,
    /// The parent's state hash at the fork — also this branch's starting
    /// hash, so a broken chain is detectable.
    pub state_hash: u64,
    /// The tick the fork happened at.
    pub tick: u64,
}

/// One branch's sidecar record: ancestry plus its own action log.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BranchRecord {
    pub format: u32,
    /// `None` for a root run (a fresh world, not forked from anything).
    pub parent: Option<ParentRef>,
    /// Player actions applied on this branch since the fork, in apply
    /// order (non-decreasing ticks). The UI appends every record it emits;
    /// for a scripted headless run this is the script.
    pub actions: Vec<PlayerAction>,
}

impl BranchRecord {
    /// A fresh root record: no ancestry, empty log.
    pub fn root() -> Self {
        BranchRecord {
            format: BRANCH_FORMAT,
            parent: None,
            actions: Vec::new(),
        }
    }

    /// The record a fork of `parent` creates: ancestry pointing at the
    /// parent's save with its exact state hash and tick, empty log.
    pub fn fork_of(parent_save: &str, parent: &WorldSnapshot) -> Self {
        BranchRecord {
            format: BRANCH_FORMAT,
            parent: Some(ParentRef {
                save: parent_save.to_string(),
                state_hash: parent.state_hash(),
                tick: parent.tick,
            }),
            actions: Vec::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self, SaveError> {
        let text = std::fs::read_to_string(path).map_err(SaveError::Io)?;
        let record: BranchRecord =
            ron::from_str(&text).map_err(|e| SaveError::Corrupt(e.to_string()))?;
        record.validate()?;
        Ok(record)
    }

    pub fn save(&self, path: &Path) -> Result<(), SaveError> {
        self.validate()?;
        let pretty = ron::ser::PrettyConfig::default();
        let text = ron::ser::to_string_pretty(self, pretty)
            .map_err(|e| SaveError::Corrupt(e.to_string()))?;
        std::fs::write(path, text).map_err(SaveError::Io)
    }

    /// Fork `parent_save` into a new branch: copy the save to `child_save`
    /// and write a fresh ancestry record at `record_path`. This is the
    /// operation behind `genesis branch`, and the Phase 6 UI performs
    /// exactly these steps.
    ///
    /// Refuses to overwrite an existing child save or record — a branch is
    /// a *new* file, and overwriting one would silently destroy that
    /// branch's state and its replay provenance (the action log). The same
    /// check rejects a self-parent fork (`child == parent`): the parent
    /// exists by definition.
    pub fn fork_save(
        parent_save: &Path,
        child_save: &Path,
        record_path: &Path,
    ) -> Result<Self, SaveError> {
        for path in [child_save, record_path] {
            if path.exists() {
                return Err(SaveError::Io(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!(
                        "refusing to overwrite {} — a branch must be a new \
                         file (delete it first to reuse the name)",
                        path.display()
                    ),
                )));
            }
        }
        let snap = crate::load_from_file(parent_save)?;
        crate::save_to_file(&snap, child_save)?;
        let record = BranchRecord::fork_of(&parent_save.display().to_string(), &snap);
        record.save(record_path)?;
        Ok(record)
    }

    pub fn validate(&self) -> Result<(), SaveError> {
        if self.format != BRANCH_FORMAT {
            return Err(SaveError::UnsupportedVersion(self.format, BRANCH_FORMAT));
        }
        let mut last = 0u64;
        for (i, a) in self.actions.iter().enumerate() {
            a.action
                .validate()
                .map_err(|e| SaveError::Corrupt(format!("branch log action {i}: {e}")))?;
            if a.tick < last {
                return Err(SaveError::Corrupt(format!(
                    "branch log action {i} at tick {} after tick {last} — the log \
                     is apply order, ticks never decrease",
                    a.tick
                )));
            }
            last = a.tick;
        }
        if let (Some(p), Some(first)) = (&self.parent, self.actions.first())
            && first.tick < p.tick
        {
            return Err(SaveError::Corrupt(format!(
                "branch log starts at tick {} before the fork tick {} — \
                 those actions belong to the parent's log",
                first.tick, p.tick
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use genesis_config::{ActionKind, RegionSpec, SimConfig};
    use genesis_sim::Simulation;

    fn action(tick: u64, value: f32) -> PlayerAction {
        PlayerAction {
            tick,
            action: ActionKind::FieldSet {
                field: 0,
                region: RegionSpec {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 64.0,
                    y1: 64.0,
                },
                value,
            },
        }
    }

    fn tmp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "genesis-branch-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn small_config() -> SimConfig {
        SimConfig {
            particle_count: 200,
            world_width: 128.0,
            world_height: 128.0,
            ..SimConfig::default()
        }
    }

    #[test]
    fn record_roundtrips_through_ron() {
        let dir = tmp_dir();
        let path = dir.join("child.branch.ron");
        let record = BranchRecord {
            format: BRANCH_FORMAT,
            parent: Some(ParentRef {
                save: "runs/parent.gens".into(),
                state_hash: 0xdead_beef_cafe_f00d,
                tick: 500,
            }),
            actions: vec![action(600, 1.0), action(600, 2.0), action(900, 0.5)],
        };
        record.save(&path).unwrap();
        assert_eq!(BranchRecord::load(&path).unwrap(), record);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn validate_rejects_broken_records() {
        // Unknown format.
        let mut r = BranchRecord::root();
        r.format = 99;
        assert!(matches!(
            r.validate(),
            Err(SaveError::UnsupportedVersion(99, BRANCH_FORMAT))
        ));

        // Decreasing ticks (not apply order).
        let mut r = BranchRecord::root();
        r.actions = vec![action(100, 1.0), action(50, 1.0)];
        assert!(r.validate().is_err());

        // Log reaching before the fork.
        let mut r = BranchRecord::root();
        r.parent = Some(ParentRef {
            save: "p.gens".into(),
            state_hash: 1,
            tick: 500,
        });
        r.actions = vec![action(100, 1.0)];
        assert!(r.validate().is_err());

        // Structurally invalid action.
        let mut r = BranchRecord::root();
        r.actions = vec![action(100, f32::NAN)];
        assert!(r.validate().is_err());
    }

    #[test]
    fn fork_captures_parent_identity_and_the_child_continues_it() {
        // Run a parent, fork it, and prove: (a) the record pins the parent's
        // exact hash and tick; (b) parent-resumed and child both reach the
        // identical future; (c) a live edit on the child diverges the child
        // only — the branches are independent.
        let dir = tmp_dir();
        let config = small_config();
        let mut parent = Simulation::new(&config);
        for _ in 0..50 {
            parent.tick();
        }
        let snap = parent.snapshot();
        let parent_path = dir.join("parent.gens");
        crate::save_to_file(&snap, &parent_path).unwrap();

        let record = BranchRecord::fork_of(parent_path.to_str().unwrap(), &snap);
        assert_eq!(record.parent.as_ref().unwrap().tick, 50);
        assert_eq!(
            record.parent.as_ref().unwrap().state_hash,
            snap.state_hash()
        );
        assert!(record.actions.is_empty());

        // The fork is the same universe: both continuations agree.
        let child_path = dir.join("child.gens");
        crate::save_to_file(&snap, &child_path).unwrap();
        let mut resumed_parent =
            Simulation::from_snapshot(&crate::load_from_file(&parent_path).unwrap());
        let mut child = Simulation::from_snapshot(&crate::load_from_file(&child_path).unwrap());
        for _ in 0..50 {
            resumed_parent.tick();
            child.tick();
        }
        assert_eq!(
            resumed_parent.state_hash(),
            child.state_hash(),
            "an untouched fork must continue the parent's exact future"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn fork_save_creates_the_child_and_never_overwrites_one() {
        let dir = tmp_dir();
        let config = small_config();
        let mut parent = Simulation::new(&config);
        for _ in 0..10 {
            parent.tick();
        }
        let snap = parent.snapshot();
        let parent_path = dir.join("parent.gens");
        crate::save_to_file(&snap, &parent_path).unwrap();

        let child_path = dir.join("child.gens");
        let record_path = dir.join("child.gens.branch.ron");
        let rec = BranchRecord::fork_save(&parent_path, &child_path, &record_path).unwrap();
        assert_eq!(rec.parent.as_ref().unwrap().state_hash, snap.state_hash());
        assert_eq!(
            crate::load_from_file(&child_path).unwrap().state_hash(),
            snap.state_hash(),
            "the child save must be the parent's exact state"
        );
        assert_eq!(BranchRecord::load(&record_path).unwrap(), rec);

        // The child now exists — with an action appended to its log, the
        // record carries provenance that an overwrite would destroy.
        let mut lived = rec.clone();
        lived.actions.push(action(20, 1.0));
        lived.save(&record_path).unwrap();
        let err = BranchRecord::fork_save(&parent_path, &child_path, &record_path)
            .expect_err("re-forking onto an existing branch must fail");
        assert!(matches!(err, SaveError::Io(ref e)
            if e.kind() == std::io::ErrorKind::AlreadyExists));
        assert_eq!(
            BranchRecord::load(&record_path).unwrap(),
            lived,
            "a refused fork must leave the existing record untouched"
        );

        // A self-parent fork is refused for the same reason: the "child"
        // already exists (it is the parent).
        let self_record = dir.join("parent.gens.branch.ron");
        let err = BranchRecord::fork_save(&parent_path, &parent_path, &self_record)
            .expect_err("self-parent fork must fail");
        assert!(matches!(err, SaveError::Io(ref e)
            if e.kind() == std::io::ErrorKind::AlreadyExists));
        assert!(!self_record.exists(), "no record for a refused fork");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn branches_diverge_independently() {
        // Two forks of one save: an action on one never touches the other,
        // and the divergence is exactly the action (the no-action twin still
        // matches a straight continuation).
        let mut config = small_config();
        config.env = genesis_config::EnvSpec {
            cols: 8,
            rows: 8,
            fields: vec![genesis_config::EnvFieldSpec {
                name: String::new(),
                init: genesis_config::FieldInit::Uniform(0.0),
                dynamics: Default::default(),
            }],
        };
        let mut parent = Simulation::new(&config);
        for _ in 0..20 {
            parent.tick();
        }
        let snap = parent.snapshot();

        let mut a = Simulation::from_snapshot(&snap);
        let mut b = Simulation::from_snapshot(&snap);
        b.queue_action(action(30, 2.0)).unwrap();
        for _ in 0..30 {
            a.tick();
            b.tick();
        }
        parent.tick(); // the original also continues
        for _ in 0..29 {
            parent.tick();
        }
        assert_eq!(
            a.state_hash(),
            parent.state_hash(),
            "the untouched branch must match a straight continuation"
        );
        assert_ne!(
            a.state_hash(),
            b.state_hash(),
            "the edited branch must diverge"
        );
    }
}
