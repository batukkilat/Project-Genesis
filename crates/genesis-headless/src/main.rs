//! Headless CLI for Project Genesis.
//!
//! The simulation runs to completion here with no renderer and no AI —
//! constitution rule 3. This binary is also the determinism test bench.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use clap::{Parser, Subcommand};
use genesis_config::SimConfig;
use genesis_sim::Simulation;

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
    },
    /// Verify determinism: two fresh runs, plus a save/resume run, must all
    /// produce the same final state hash. Exits non-zero on divergence.
    Verify {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long, default_value_t = 1000)]
        ticks: u64,
    },
    /// Measure tick throughput.
    Bench {
        #[arg(long, default_value_t = 1_000_000)]
        particles: u64,
        #[arg(long, default_value_t = 600)]
        ticks: u64,
    },
    /// Write a default config file to the given path.
    InitConfig { path: PathBuf },
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

fn run(cli: Cli) -> Result<ExitCode, Box<dyn std::error::Error>> {
    match cli.command {
        Command::Run {
            config,
            ticks,
            load,
            save,
            hash_every,
        } => {
            let mut sim = match load {
                Some(path) => Simulation::from_snapshot(&genesis_persist::load_from_file(&path)?),
                None => Simulation::new(&load_config(&config)?),
            };

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

        Command::Verify { config, ticks } => {
            let config = load_config(&config)?;

            let run_hash = |ticks: u64| {
                let mut sim = Simulation::new(&config);
                for _ in 0..ticks {
                    sim.tick();
                }
                (sim.state_hash(), sim)
            };

            let (hash_a, _) = run_hash(ticks);
            let (hash_b, _) = run_hash(ticks);

            // Save/resume path: run half, save, reload, finish.
            let half = ticks / 2;
            let (_, mut sim_c) = run_hash(half);
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

            println!("run A        {hash_a:#018x}");
            println!("run B        {hash_b:#018x}");
            println!("save/resume  {hash_c:#018x}");

            if hash_a == hash_b && hash_a == hash_c {
                println!("DETERMINISTIC over {ticks} ticks");
                Ok(ExitCode::SUCCESS)
            } else {
                eprintln!("DIVERGED: replay is not deterministic");
                Ok(ExitCode::FAILURE)
            }
        }

        Command::Bench { particles, ticks } => {
            let config = SimConfig {
                particle_count: particles,
                ..Default::default()
            };

            let start = Instant::now();
            let mut sim = Simulation::new(&config);
            let spawn_time = start.elapsed();

            let start = Instant::now();
            for _ in 0..ticks {
                sim.tick();
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
            println!("state hash   {:#018x}", sim.state_hash());
            Ok(ExitCode::SUCCESS)
        }

        Command::InitConfig { path } => {
            SimConfig::default().save(&path)?;
            println!("wrote default config to {}", path.display());
            Ok(ExitCode::SUCCESS)
        }
    }
}
