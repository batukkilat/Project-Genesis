# USS
Purpose: Define WHAT reality is.

Primitives:
Particle, Matter, Energy, Information, Time, Interaction.

Particle:
position, velocity, matter, energy, information, memory, bonds, state.
(Field introduction per phase: see spec/Particle.md.)

Space: 2D continuous positions (f32) on a torus (both axes wrap). Discrete chunk partitioning is an implementation detail; environment fields sample on discrete grids.
Time: deterministic fixed-dt ticks. dt never changes at runtime — time warp means more ticks per wall second, never a bigger dt.
Units: abstract simulation units, internally consistent. No SI.
Matter: conserved (physics mode).
Energy: conserved (physics mode).
Information: first-class quantity. Scalar carrier in Phase 1; semantics defined in Phase 3.

Dynamics, two kinds:
Continuous (Physics): motion, diffusion, heat — generic hardcoded operators, config-parameterized.
Discrete (Interaction): Condition -> Probability -> Cost -> Action (transfers, bonds, create/destroy).

Emergence:
Everything above Particle must emerge.

Observer:
Outside simulation. Read-only. Detects possible life/intelligence/civilization. Never affects state.

AI:
Interprets observations only.
