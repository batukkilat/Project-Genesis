# QUESTIONS — decisions parked for the user

Open design forks the autonomous loop must not decide alone. Answer inline
(edit this file or just say it in chat); answered items move to the ROADMAP
decisions log and get deleted from here.

## Q1 — Information semantics (blocks: interaction rules that use information)

What does a particle's `information` scalar actually DO? Most emergence-critical
decision in the project (ROADMAP open question #1).

- **A (recommended): condition-gate + lossy copy.** Information is readable by
  rule conditions (rules can require `information >= x`) and copyable between
  particles through interactions at an energy cost, with configurable copy
  noise. Not conserved (it can be created by paying energy, lost by decay).
  Why: this is the minimal semantics that makes self-replication *possible*
  but not built-in — a structure that maintains and copies its information
  against decay is exactly the kind of thing the Observer should hunt for.
- **B: conserved scalar like matter.** Strict conservation, transfers only.
  Simpler, but information that can't be created or copied caps emergence
  hard — replication becomes zero-sum.
- **C: defer.** Phase 3 ships matter/energy transfers only; information stays
  inert until Phase 3.5. Zero risk, but the interaction engine's design should
  know the answer before the rule schema freezes.

## Q2 — Rule authoring format (blocks: rule loader, rule packs)

ROADMAP open question #3. Internal compiled representation is being built
either way; this is only about what authors write.

- **A (recommended): RON data schema.** Declarative conditions (quantity
  comparisons, bond counts, neighbor requirements), fixed action vocabulary
  (transfer, bond, unbond, split, merge, emit, absorb). Compiled + validated
  at load, whole pack hashed into replay identity. Why: data stays hashable,
  diffable, sandboxable; no interpreter in the hot loop; matches "rules are
  content, not code".
- **B: embedded scripting (rhai/lua) for conditions.** Maximum expressiveness,
  but script execution in the pair loop kills performance, and replay identity
  of a Turing-complete rule is much weaker.

## Q3 — Bond storage layout (blocks: bonds, compound structures)

ROADMAP open question #5. Research done: docs/research/bond-storage.md
(MD-literature survey — LAMMPS/GROMACS solve exactly this).

- **A (recommended): SoA edge list + per-tick CSR mirror.** Master edge list
  stores stable particle ids (a < b) + per-bond state, canonically sorted by
  (a, b) — this is what saves/hashes. Each tick, after particle canonicalize,
  rebuild a CSR adjacency (both directions, keyed by particle index) that the
  passes iterate — contiguous per-particle bond slices, disjoint writes, drops
  straight into the existing parallel model. id→index via a per-tick rebuilt
  hash map used lookup-only (hash order never leaks into results). Destroyed
  particles self-heal: edges whose endpoint no longer resolves get pruned
  during rebuild.
- **B: edge-list only.** Simpler, but the force pass would write both
  endpoints of each bond — needs atomics or reductions, which breaks the
  thread-count-invariance rule.
- **C: fixed per-particle bond slots.** Requires a hard cap on bonds per
  particle — hidden emergence ceiling, rejected by research.
