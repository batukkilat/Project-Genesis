# Physics
Purpose: Continuous dynamics. Hardcoded generic operators, parameterized by config — the fast path for millions of particles.

Split vs Interactions:
Physics = continuous, every-tick operators (movement, forces, diffusion, heat, pressure, gravity).
Interactions = discrete rule-driven events (see spec/Interaction.md).
Operators are generic and configurable; no Earth constants baked in.

Responsibilities:
- movement (fixed-timestep integration)
- forces
- collisions
- diffusion
- heat
- pressure
- gravity

World:
2D torus (both axes wrap). No edges, no boundary artifacts.

Modes:
Physics:
- conserve matter
- conserve energy
- conservation checked per tick within stated f32 tolerance; accounting uses compensated summation

Sandbox:
- rules configurable

Requirements:
- deterministic (same build + platform)
- multithread-safe (chunk-parallel, two-phase collect-then-commit)
- adaptive detail keyed off simulation state only, never wall clock or framerate
- renderer independent
- headless

AI Checklist:
[ ] No Earth assumptions
[ ] Modular systems
[ ] Single-thread vs multi-thread hash-identical
