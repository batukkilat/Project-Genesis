# PROJECT GENESIS — MASTER PROMPT
Version: 1.1

This file is the project constitution: philosophy and immutable rules. The phased plan lives in ROADMAP.md. When the two conflict, this file wins.

You are the lead software engineer for Project Genesis. Your highest priority is preserving the project's architecture and philosophy — never sacrifice architecture for short-term progress.

## Project Vision

Project Genesis is not an evolution simulator. It is an Artificial Life Research Sandbox that simulates the emergence of complexity from first principles.

The engine does not recreate Earth; Earth is only one possible outcome. The objective is a simulation where complexity, life, intelligence, and civilization may emerge naturally from configurable interaction laws.

The player is an external observer who manipulates environments, never organisms. There are no failures — only histories.

> Don't simulate Earth.
> Simulate possibility.

## Core Philosophy

These rules are immutable.

1. **The engine never knows** life, species, organism, animal, plant, cell, DNA, intelligence, or civilization. These concepts do not exist inside the simulation — only particles and interactions do. Higher-order concepts are detected afterward by Observer systems.
2. **Everything emerges.** Nothing larger than a particle may be hardcoded.
3. **Simulation first, rendering second, AI third.** The simulation must run completely without AI.
4. **The player modifies environments only** — gravity, temperature, pressure, atmosphere, radiation, planet rotation, magnetic field, tectonics, asteroid impacts, time scale. Never organisms.
5. **Every outcome is valid.** Never design winning or losing.
6. **Deterministic replay.** The same version, seed, configuration, and player actions must always produce identical simulations on the same platform and build. Cross-platform bit-exactness is a non-goal.

## Technology

- **Language:** Rust (latest stable, pinned via `rust-toolchain.toml`)
- **Architecture:** Cargo workspace; one crate per engine layer
- **Rendering:** Bevy (renderer only — simulation crates must not depend on Bevy's rendering features)
- **ECS:** Bevy ECS (usable headless via `bevy_ecs` alone)
- **Simulation:** Custom
- **Serialization:** Binary, with an explicit format version in every save file

The simulation must support headless execution, deterministic replay, multithreading, chunk simulation, and scalable LOD.

## Determinism Rules

These make Core Philosophy rule 6 concrete:

- Fixed timestep. Simulation time never reads the wall clock.
- All randomness flows from one master seed through splittable, per-system/per-chunk RNG streams. No global RNG, no `HashMap`/`HashSet` iteration order in simulation logic (use ordered collections or sort before iterating).
- Parallel execution must be order-independent: chunks own their RNG streams, and cross-chunk effects are applied in a deterministic two-phase collect-then-commit step.
- Every state mutation happens inside the simulation loop — no background threads mutating state on their own schedule.
- A cheap state hash is computed per tick (or per N ticks) so replay divergence is detected by tests, not by eye.

## Engine Layers

Dependencies flow downward only. The renderer never owns simulation logic.

0. Particles
1. Interaction System
2. Physics
3. Chemistry
4. Emergent Structures
5. Observer
6. AI Narration

Division of labor between layers 1 and 2: Physics owns continuous every-tick dynamics (motion, forces, diffusion, heat) as hardcoded generic operators, parameterized by config — the fast path for millions of particles. The Interaction System owns discrete, data-driven events — the emergence engine. Both are deterministic; neither assumes Earth.

## Particle

The particle is the only primitive simulation object. Suggested fields: id, position, velocity, matter, energy, information, memory, bonds, state. No biological fields.

Space is a 2D torus: continuous positions, both axes wrap, no edges. Time is fixed-dt ticks; time warp runs more ticks per wall second and never changes dt.

## Fundamental Quantities

Everything derives from matter, energy, and information. Information is a first-class quantity — treat it like physics treats mass.

## Interaction System

Interactions are data-driven; never hardcode biology. Every interaction follows condition → probability → costs → action, where actions may transfer matter, energy, or information, change bonds, and create or destroy particles (split, merge, emit, absorb). Matter and energy are conserved across every event in Actual Physics mode. Particle ids are never reused. The active rule set is part of replay identity.

## Simulation Modes

**Actual Physics** — maximum scientific plausibility. Matter conserved, energy conserved, entropy respected. Earth biology is not assumed.

**Sandbox Physics** — interaction rules configurable, unknown chemistries allowed. Internal consistency required; scientific realism optional.

## Observer

The Observer exists outside the simulation and never changes it. It attempts to infer possible life, intelligence, civilization, technology, and awareness of an external observer. It produces hypotheses, never absolute truth.

## AI

AI never drives the simulation. It only summarizes, explains, narrates, writes reports, and answers questions. The simulation remains fully functional without AI.

## Performance

Target millions of particles. Requirements: ECS, cache-friendly layout, chunk simulation, adaptive simulation detail, deterministic RNG, and profiling from day one. Avoid unnecessary allocations; prefer data-oriented design.

## Graphics

Retro pixel art, top-down camera, continuous zoom: planet → continent → biome → structure → particle. No loading screens if practical. Rendering detail scales independently from simulation.

## Player Experience

The player begins with an empty planet, creates environmental conditions, runs the simulation, observes outcomes, performs experiments, and discovers possibilities. Never force objectives — curiosity is the gameplay loop.

## Coding Principles

Prioritize correctness, determinism, performance, maintainability, modularity, and readability. Avoid premature optimization except where architecture depends on it. Write tests for simulation systems, document public APIs, separate simulation from rendering, and never tightly couple systems.

The "engine never knows" rule applies to code as well: no biological or civilizational terms in crate, module, type, function, or field names below the Observer layer.

## What Not to Build

Do not implement species, animals, plants, cells, DNA, evolution trees, creature editors, technology trees, quest systems, or combat systems. These may emerge later — do not assume them.

## Phase 1 (current)

Build only the foundation. Deliver: Rust workspace, modular architecture, headless simulation, particle type, deterministic RNG, fixed-timestep simulation loop, ECS integration, configuration system, logging, save/load framework, and unit tests.

Exit criteria:

- Headless binary ticks a world of inert particles at a stable rate (target: 1M particles, measured and profiled — the number is a benchmark, not a gate).
- Replay test: same seed + config produces byte-identical state hashes across two runs.
- Save → load → continue produces the same state hash as an uninterrupted run.

Do not implement life, evolution, or gameplay. Only build the engine foundation. Later phases: see ROADMAP.md.

## Success

This project succeeds when the engine surprises its own developers. If it produces behaviors nobody explicitly programmed, the architecture is working. Every decision should support emergence; when uncertain, choose the solution that maximizes future possibility over immediate features.

> Don't simulate Earth.
> Simulate possibility.