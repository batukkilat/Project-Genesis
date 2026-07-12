# Planet rotation on a 2D torus — research and decision (Q-2026-07-10-B)

Owner asked (2026-07-12) for a literature check before settling the parked
rotation fork. Findings below; decision at the end.

## What the physics literature says

**The f-plane precedent.** Geophysical fluid dynamics has faced exactly this
question — how to put planetary rotation into a flat 2D domain — and its
standard answer is the *f-plane approximation*: keep only the Coriolis term
with a constant parameter `f = 2Ω`, drop the centrifugal term (it is absorbed
into the static potential on a real planet, and on a torus there is no center
for it to point at) and drop curvature. Rotation in a flat periodic 2D world
*is* a uniform transverse acceleration `a = 2Ω · perp(v)` — this is not an
approximation invented for Genesis, it is how planetary rotation is modeled in
2D by the whole field ([MIT OCW Coriolis essay](https://ocw.mit.edu/courses/res-12-001-topics-in-fluid-dynamics-fall-2024/mitres_12_001_f24_essay3_pt1.pdf),
[Price, *A Coriolis tutorial*](https://www2.whoi.edu/staff/jprice/wp-content/uploads/sites/199/2022/03/aCt-P1-V9.pdf)).

**The conservation objection dissolves on inspection.**

- *Energy is exactly conserved.* The Coriolis force is always perpendicular
  to velocity, so it does no work — it deflects, never speeds up or slows
  down ([Physics LibreTexts 12.8](https://phys.libretexts.org/Bookshelves/Classical_Mechanics/Variational_Principles_in_Classical_Mechanics_(Cline)/12:_Non-inertial_Reference_Frames/12.08:_Coriolis_Force),
  [Work and energy in rotating systems](https://arxiv.org/pdf/1201.2971)).
  The constitution's "matter and energy conserved" survives untouched.
- *Momentum is not lost, it rotates.* The Coriolis term is mathematically
  identical to a uniform magnetic field acting on unit charge (the classic
  Coriolis–Lorentz analogy, [Royer 2011](https://arxiv.org/pdf/1109.3624)).
  Summing `m·2Ω·perp(v)` over particles: `dP/dt = 2Ω·perp(P)` — the total
  momentum vector rotates at constant magnitude with angular frequency `2Ω`.
  So the carve-out is small and checkable: **|P| stays conserved**, and P's
  direction advances by a known angle per tick. Conservation accounting keeps
  a real invariant instead of an exemption.

**The emergence payoff is exactly what the project wants.** Rotation is one
of the best-documented pattern generators in 2D dynamics:

- 2D turbulence under rotation feeds an inverse cascade — energy piles up at
  large scales and condenses into long-lived coherent vortices
  ([PRX 6, 041036](https://journals.aps.org/prx/abstract/10.1103/PhysRevX.6.041036),
  [Kolokolov & Lebedev](https://arxiv.org/pdf/1511.03113),
  [rotating-tank experiments](https://arxiv.org/pdf/1412.3933)).
- Chiral active matter — *particle-based* systems with transverse forces, the
  closest literature analog to our sim — self-organizes into vortex arrays,
  circling crystals, and rotating pattern phases
  ([Liebchen & Levis, Chiral active matter](https://www.researchgate.net/publication/363285730_Chiral_active_matter),
  [pattern formation with chiral interactions](https://arxiv.org/html/2209.05454v3),
  [self-reverting vortices](https://www.nature.com/articles/s42005-024-01637-2)).
- With a *gradient* in the rotation parameter (β-plane), 2D flows
  spontaneously form alternating zonal jets at the Rhines scale — Jupiter's
  bands ([Farrell & Ioannou](https://arxiv.org/pdf/1208.5665),
  [Rhines-scale spectra](https://epic.awi.de/id/eprint/10282/)). A uniform
  `spin` is the f-plane; a future spatially varying spin is the β-plane and
  buys banded structure for free. Noted as a possible extension, not built.

**Numerics: exact rotation, not an explicit force term.** Adding
`2Ω·perp(v)·dt` as an explicit Euler force inflates speed by
`sqrt(1 + (2Ω dt)²)` per tick — systematic energy injection. Plasma PIC codes
solved this decades ago (the Boris pusher): apply the magnetic/Coriolis part
as an *exact rotation* of the velocity vector. We do the same: after the force
kick, rotate `v` by `θ = -2·spin·dt` (one `sin_cos` per tick, two fused
multiplies per particle). Speed is preserved to f32 rounding, unconditionally
stable at any spin, deterministic, thread-count invariant (pure per-particle
map). ([High-accuracy particle motion in static magnetic fields](https://arxiv.org/pdf/2604.20876) surveys why rotation-based pushers beat explicit ones.)

## Why not option B (insolation cycling)

B is a periodic driver for env fields — a *field-dynamics extension*
(oscillating relax target), not a reading of "rotation": particles never feel
it, and it quietly reintroduces the source concept cut from field dynamics v1.
It stays available later as its own generic feature when content wants
day/night forcing. Nothing about A blocks it.

## Decision (Q-2026-07-10-B → option C, staged; A implemented now)

- New physics param `spin` (f32, default 0.0, finite, either sign): angular
  velocity Ω of the world frame. Applied in integrate as an exact velocity
  rotation by `-2·spin·dt` after the force kick, active particles only (LOD
  gate unchanged — frozen particles stay bit-identical).
- Replay identity: hashes **only when non-zero** (the LOD/env/dynamics
  precedent — a spin-0 world is byte-identical to a pre-spin world and must
  keep its exact hash). Save format v15 writes it unconditionally.
- Player verb `SpinSet { spin }`: sets the param at its stamped tick through
  the one action path (scripted = recorded = live). Pending SpinSet hashes
  like any pending action; an applied SpinSet is state (the spin param).
- Conservation contract with spin ≠ 0: matter exact (untouched), energy exact
  (rotation does no work), **|P| conserved** while P's direction rotates at
  `2·spin` — tests assert the magnitude invariant.
- Future, not now: spatially varying spin (β-plane → zonal jets) as a
  possible Phase 7+ extension; insolation cycling as a field-dynamics
  oscillator when content demands it.
