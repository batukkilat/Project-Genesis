//! Headless CLI for Project Genesis.
//!
//! The simulation runs to completion here with no renderer and no AI —
//! constitution rule 3. This binary is also the determinism test bench.

mod analysis;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use clap::{Parser, Subcommand};
use genesis_config::{ActionScript, RulePack, SimConfig};
use genesis_sim::Simulation;
use genesis_sim::interact::RuleSet;

#[derive(Parser)]
#[command(name = "genesis", about = "Project Genesis headless simulation")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a simulation for N ticks and print the final state hash.
    Run {
        /// RON config file; defaults are used when omitted.
        #[arg(long)]
        config: Option<PathBuf>,
        /// RON rule pack; no interactions when omitted. Part of replay
        /// identity — a different pack is a different universe.
        #[arg(long)]
        rules: Option<PathBuf>,
        /// RON player-action script: tick-stamped environment edits, applied
        /// at the start of their stamped tick. Part of replay identity while
        /// pending (Q-2026-07-08-B).
        #[arg(long)]
        actions: Option<PathBuf>,
        #[arg(long, default_value_t = 1000)]
        ticks: u64,
        /// Resume from a save file instead of creating a fresh world.
        #[arg(long)]
        load: Option<PathBuf>,
        /// Save the final state to this file.
        #[arg(long)]
        save: Option<PathBuf>,
        /// Print the state hash every N ticks.
        #[arg(long)]
        hash_every: Option<u64>,
        /// Every N ticks, print read-only structure diagnostics: bond-graph
        /// components, their persistence across samples, and quantity
        /// totals. Diagnostics never affect the simulation.
        #[arg(long)]
        report: Option<u64>,
        /// Worker threads (0 = all cores). Never changes results.
        #[arg(long, default_value_t = 0)]
        threads: usize,
    },
    /// Verify determinism: two fresh runs, a save/resume run, and a
    /// single-threaded run must all produce the same final state hash.
    /// Exits non-zero on divergence.
    Verify {
        #[arg(long)]
        config: Option<PathBuf>,
        /// RON rule pack to verify with (interactions active).
        #[arg(long)]
        rules: Option<PathBuf>,
        /// RON player-action script to verify with. A scripted run passing
        /// this four-way check is the Phase 4 exit criterion, executable.
        #[arg(long)]
        actions: Option<PathBuf>,
        #[arg(long, default_value_t = 1000)]
        ticks: u64,
    },
    /// Measure tick throughput.
    Bench {
        #[arg(long, default_value_t = 1_000_000)]
        particles: u64,
        #[arg(long, default_value_t = 120)]
        ticks: u64,
        /// RON config (physics, LOD policy, initial ranges). `--particles`
        /// always overrides the config's particle_count.
        #[arg(long)]
        config: Option<PathBuf>,
        /// RON rule pack to benchmark with (physics only when omitted).
        #[arg(long)]
        rules: Option<PathBuf>,
        /// Force the LOD policy off, whatever the config says. Lets one config
        /// bench LOD-on vs LOD-off on the identical world (apples-to-apples).
        #[arg(long)]
        no_lod: bool,
        /// Worker threads (0 = all cores). Never changes results.
        #[arg(long, default_value_t = 0)]
        threads: usize,
    },
    /// Write a default config file to the given path.
    InitConfig { path: PathBuf },
    /// Write an example rule pack to the given path.
    InitRules { path: PathBuf },
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn load_config(path: &Option<PathBuf>) -> Result<SimConfig, Box<dyn std::error::Error>> {
    match path {
        Some(p) => Ok(SimConfig::load(p)?),
        None => Ok(SimConfig::default()),
    }
}

fn load_rules(path: &Option<PathBuf>) -> Result<RuleSet, Box<dyn std::error::Error>> {
    match path {
        Some(p) => Ok(RuleSet::compile(&RulePack::load(p)?)),
        None => Ok(RuleSet::default()),
    }
}

fn load_actions(path: &Option<PathBuf>) -> Result<ActionScript, Box<dyn std::error::Error>> {
    match path {
        Some(p) => Ok(ActionScript::load(p)?),
        None => Ok(ActionScript::default()),
    }
}

/// Size the global rayon pool (0 = rayon's default, all cores). Thread count
/// is a scheduling knob only — it never appears in replay identity.
fn init_thread_pool(threads: usize) -> Result<(), Box<dyn std::error::Error>> {
    if threads > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global()?;
    }
    Ok(())
}

fn run(cli: Cli) -> Result<ExitCode, Box<dyn std::error::Error>> {
    match cli.command {
        Command::Run {
            config,
            rules,
            actions,
            ticks,
            load,
            save,
            hash_every,
            report,
            threads,
        } => {
            init_thread_pool(threads)?;
            let mut sim = match load {
                Some(path) => {
                    if rules.is_some() {
                        return Err("--rules cannot be combined with --load: the rule set \
                                    is part of the save's replay identity"
                            .into());
                    }
                    if actions.is_some() {
                        return Err("--actions cannot be combined with --load: the pending \
                                    action queue is part of the save's replay identity"
                            .into());
                    }
                    Simulation::from_snapshot(&genesis_persist::load_from_file(&path)?)
                }
                None => Simulation::with_rules_and_actions(
                    &load_config(&config)?,
                    load_rules(&rules)?,
                    load_actions(&actions)?,
                ),
            };

            // A structure counts as persistent once it has survived
            // PERSIST_AFTER consecutive report samples.
            const PERSIST_AFTER: u32 = 5;
            let mut tracker = analysis::StructureTracker::new(PERSIST_AFTER);

            let start = Instant::now();
            for i in 1..=ticks {
                sim.tick();
                if let Some(every) = hash_every
                    && every > 0
                    && i % every == 0
                {
                    println!(
                        "tick {:>12}  hash {:#018x}",
                        sim.tick_count(),
                        sim.state_hash()
                    );
                }
                if let Some(every) = report
                    && every > 0
                    && i % every == 0
                {
                    let snap = sim.snapshot();
                    let comps = analysis::bond_components(&snap);
                    let stats = analysis::sample_stats(&snap, &comps);
                    let track = tracker.observe(&comps);
                    println!(
                        "tick {:>10}  n {:>7}  bonds {:>6}  comps {:>5} (largest {:>4}, \
                         in-multi {:>6})  persist>={PERSIST_AFTER} {:>4} (oldest {:>3})  \
                         M {:.3}  E {:.3}  I {:.3}",
                        stats.tick,
                        stats.particles,
                        stats.bonds,
                        stats.components,
                        stats.largest_component,
                        stats.in_multi,
                        track.persistent,
                        track.oldest_age,
                        stats.total_matter,
                        stats.total_energy,
                        stats.total_information,
                    );
                }
            }
            let elapsed = start.elapsed();

            println!("final tick   {}", sim.tick_count());
            println!("state hash   {:#018x}", sim.state_hash());
            println!(
                "throughput   {:.0} ticks/s",
                ticks as f64 / elapsed.as_secs_f64().max(f64::EPSILON)
            );

            if let Some(path) = save {
                genesis_persist::save_to_file(&sim.snapshot(), &path)?;
                println!("saved to     {}", path.display());
            }
            Ok(ExitCode::SUCCESS)
        }

        Command::Verify {
            config,
            rules,
            actions,
            ticks,
        } => {
            let config = load_config(&config)?;
            let rule_set = load_rules(&rules)?;
            let script = load_actions(&actions)?;

            let run_hash = |ticks: u64| {
                let mut sim =
                    Simulation::with_rules_and_actions(&config, rule_set.clone(), script.clone());
                for _ in 0..ticks {
                    sim.tick();
                }
                (sim.state_hash(), sim)
            };

            let (hash_a, _) = run_hash(ticks);
            let (hash_b, _) = run_hash(ticks);

            // Single-threaded run: thread count must never change results.
            let single_pool = rayon::ThreadPoolBuilder::new().num_threads(1).build()?;
            let (hash_s, _) = single_pool.install(|| run_hash(ticks));

            // Save/resume path: run half, save, reload, finish.
            let half = ticks / 2;
            let (_, sim_c) = run_hash(half);
            let mut bytes = Vec::new();
            genesis_persist::save_to_writer(&sim_c.snapshot(), &mut bytes)?;
            drop(sim_c);
            let mut resumed = Simulation::from_snapshot(&genesis_persist::load_from_reader(
                &mut bytes.as_slice(),
            )?);
            for _ in 0..(ticks - half) {
                resumed.tick();
            }
            let hash_c = resumed.state_hash();

            // LOD-on and LOD-off are different universes by design; the
            // four-way check proves the *given* policy is self-identical across
            // threads and save/resume, not that it matches a LOD-off run.
            let lod_mode = if config.lod.enabled {
                format!(
                    "on (chunk_cells={}, {} rungs, max_rate={})",
                    config.lod.chunk_cells,
                    config.lod.ladder.len(),
                    config.lod.max_rate()
                )
            } else {
                "off".to_string()
            };
            println!("lod          {lod_mode}");
            println!("run A        {hash_a:#018x}");
            println!("run B        {hash_b:#018x}");
            println!("save/resume  {hash_c:#018x}");
            println!("1-thread     {hash_s:#018x}");

            if hash_a == hash_b && hash_a == hash_c && hash_a == hash_s {
                println!("DETERMINISTIC over {ticks} ticks");
                Ok(ExitCode::SUCCESS)
            } else {
                eprintln!("DIVERGED: replay is not deterministic");
                Ok(ExitCode::FAILURE)
            }
        }

        Command::Bench {
            particles,
            ticks,
            config,
            rules,
            no_lod,
            threads,
        } => {
            init_thread_pool(threads)?;
            let mut config = load_config(&config)?;
            // `--particles` is the authority for scale; the config supplies
            // physics, initial ranges, and the LOD policy.
            config.particle_count = particles;
            if no_lod {
                config.lod.enabled = false;
            }
            println!("threads      {}", rayon::current_num_threads());
            println!(
                "lod          {}",
                if config.lod.enabled { "on" } else { "off" }
            );

            let start = Instant::now();
            let mut sim = Simulation::with_rules(&config, load_rules(&rules)?);
            let spawn_time = start.elapsed();

            // Sample the active fraction over the run: LOD's whole point is
            // skipping quiet particles, so report how many it actually skips.
            let mut active_samples = 0u64;
            let mut active_total = 0u128;
            let start = Instant::now();
            for _ in 0..ticks {
                sim.tick();
                if let Some(active) = sim.active_count() {
                    active_total += active as u128;
                    active_samples += 1;
                }
            }
            let elapsed = start.elapsed();
            let tps = ticks as f64 / elapsed.as_secs_f64().max(f64::EPSILON);

            println!("particles    {particles}");
            println!("spawn time   {spawn_time:?}");
            println!("ticks        {ticks} in {elapsed:?}");
            println!(
                "throughput   {tps:.1} ticks/s ({:.2e} particle-ticks/s)",
                tps * particles as f64
            );
            if active_samples > 0 {
                let avg_active = active_total as f64 / active_samples as f64;
                println!(
                    "active frac  {:.3} ({:.0} of {} particles per tick, mean)",
                    avg_active / particles as f64,
                    avg_active,
                    particles
                );
            }
            println!("state hash   {:#018x}", sim.state_hash());
            Ok(ExitCode::SUCCESS)
        }

        Command::InitConfig { path } => {
            SimConfig::default().save(&path)?;
            println!("wrote default config to {}", path.display());
            Ok(ExitCode::SUCCESS)
        }

        Command::InitRules { path } => {
            RulePack::example().save(&path)?;
            println!("wrote example rule pack to {}", path.display());
            Ok(ExitCode::SUCCESS)
        }
    }
}
