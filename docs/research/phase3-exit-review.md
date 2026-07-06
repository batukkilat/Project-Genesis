# Phase 3 exit-criteria review

Date: 2026-07-06. Engine: v0.2.0 workspace at the commit adding this file.

ROADMAP Phase 3 exit criteria:

> a nontrivial rule pack produces persistent multi-particle structures
> nobody explicitly coded; determinism and conservation still hold.

## Method

Read-only diagnostics (`genesis run --report N`, added for this review)
sample the canonical snapshot every N ticks and report bond-graph connected
components, their persistence (member-overlap identity tracking across
samples: a component continues an older one when they share at least half
the members of the larger), and matter/energy/information totals. Nothing
in the diagnostics can mutate simulation state — they consume snapshots.

Configuration: `docs/research/phase3-exit-config.ron` (committed) — 8 000
particles, 1024×1024 torus (~0.0076 particles/unit², ~1.5 neighbors inside
the force radius), all three quantities live at spawn,
`information_decay = 0.02`. Report interval 500 ticks; a component counts
as *persistent* at age ≥ 5 samples, i.e. its identity survived ≥ 2 000
consecutive ticks.

Packs under review: `chains.ron` (binding regime), `actual.ron`
(conservation-respecting regime), `sandbox.ron` (amplifying regime) — all
committed content, none of which names or codes any structure larger than a
pairwise rule.

## Results

TODO(fill from logs)

## Density regime note

A first attempt used 10 000 particles in a 512×512 torus (0.38/unit² —
~640× the engine-default density). In that regime `chains.ron` gels: ~90 000
bonds by tick 1 000, component count falling as everything anneals toward a
world-spanning cluster, and throughput collapsing with the neighbor counts.
That is a valid universe (every outcome is), but it measures the density
knob rather than the rules, and it is the reason the committed review config
sits at moderate density.

## Verdict

TODO(fill after results)
