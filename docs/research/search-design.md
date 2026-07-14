# Search over worlds — design for the Phase 6.5 evolutionary loop

Status: design draft, 2026-07-13; **steps 1–2 landed 2026-07-13**, forks
ratified in the ROADMAP decisions log (Q-2026-07-13-C) with two written
divergences from this draft — the circuit breaker is a deterministic
bond-count cap, not the wall-time cap proposed below, and confirmation
runs once at end-of-search over the all-time top-k, not per generation
(see the log entry for rationale). **Instrument v1.1 landed 2026-07-14**
(Q-2026-07-14-A): spec-level `mutations_per_child` (bolder steps, from
search-01 finding 1) and `confirm_bond_cap` (a per-stage cost bound, from
search-01 finding 3); ancestry sidecars record the full operator chain,
pre-v1.1 single-`op` sidecars still load. Evidence base:
docs/research/sweeps/2026-07-13-shipped-packs.md (the baseline sweep) and
docs/research/sweeps/2026-07-13-search-01.md (the first real run).
Decisions proposed here are ratified into the ROADMAP decisions log when
the implementation lands; forks that are emergence-critical are flagged.

## What the deliverable asks

ROADMAP Phase 6.5: "mutate the best-scoring configs (parameter jitter,
rule add/drop within schema) and iterate — a basic evolutionary loop over
worlds. Mutations logged so any discovered regime is reproducible from
its ancestry." The exit criterion measures discovered regimes against the
shipped corpus on persistence × complexity.

## The fitness fork (emergence-critical — the one real decision)

The baseline sweep's first finding: the headline scalar
`persistence_complexity` is maximized by total condensation — `actual`
and `bands` sit on top of the leaderboard by welding half the world into
one immortal blob with monotonically accumulating bonds (1–2M bonds,
mean degree 200+). An evolutionary loop optimizing that scalar alone
will therefore breed condensers: the plateau Phase 6.5 exists to escape,
discovered automatically and then reinforced.

What we actually want from a "high-scoring regime" — stated once,
plainly: **many distinct structures, persisting through churn, holding
information** — not one frozen lump. Candidate fitness forms:

- **(A) The raw headline scalar.** Rejected on today's evidence (breeds
  condensation) — but it stays *reported*, because the exit criterion is
  defined in terms of it.
- **(B) A product of saturating terms.** Fitness =
  `ln(1 + structures_final) × ln(1 + lifetime_peak) ×
  (1 + information_final_share)` — each term saturates, so maxing one
  axis (one giant blob: `structures_final = 1`) cannot dominate diverse
  regimes. Cheap to compute from the existing RunScore. The log on
  structure count is essential: without it, fitness breeds fragmentation
  (thousands of 2-particle pairs) instead of condensation — the opposite
  degenerate.
- **(C) Pareto / multi-objective (NSGA-style).** Principled, no scalar
  collapse, but heavier machinery (fronts, crowding) than "a basic
  evolutionary loop" warrants, and harder to explain in a findings doc.
  Premature until (B) demonstrably fails.

**Recommendation: (B)**, with the exact form a `SearchSpec` config field
(so a sweep of fitness functions is itself runnable), and the raw
headline scalar always recorded alongside — the exit criterion is judged
on the record, not on the fitness the search happened to climb.
Degenerate-regime guards (see circuit breakers) apply regardless of form.

## Mutation operators (schema-bounded, no new vocabulary)

All mutation happens on the *authoring* representations (`SimConfig`,
`RulePack`) and re-validates through the existing loaders — the search
can never express a world the schema forbids. Operators, all driven by a
seeded RNG owned by the search (never the simulation's):

1. **Parameter jitter (config):** multiply one of
   {interaction physics params, information_decay, initial-range ends}
   by `exp(u·σ)`, u uniform in [−1,1] — multiplicative, so scales stay
   positive and proportionate; re-clamped to validation bounds.
2. **Parameter jitter (rule):** same form on one rule's
   radius / probability / transfer amounts / condition bounds /
   info_copy cost-noise / emit fractions.
3. **Rule drop:** remove one rule (min 1 rule kept).
4. **Rule duplicate-and-jitter:** copy an existing rule, jitter several
   of its fields — the schema-bounded analog of gene duplication;
   "rule add" from nothing would need a rule generator, which is more
   invention than the deliverable asks for. (Flagged as the deliberate
   reading of "rule add/drop within schema".)
5. **Condition rewire:** re-point one condition bound at a different
   quantity (matter/energy/information) keeping the interval.

Env fields, action scripts, and LOD stay **fixed** in v1: mutating the
world's geometry/scale changes evaluation cost unpredictably (finding 5)
and mutating LOD changes what "lifetime" measures (finding 4). One
mutation per child keeps attribution readable in ancestry.

## Loop shape and reuse

Generation = a sweep. The search **emits an explicit SweepSpec run list**
(the one-spec-format decision, Q-2026-07-13-B), writes mutated
configs/packs into the search's output directory, invokes the existing
driver logic in-process, reads back `ScoreRecord`s, selects parents
(truncation selection: top-k by fitness), mutates, repeats. Nothing new
executes worlds; the search only authors content and reads records.

Population sizing is budget-driven: finding 5 (bonded regimes cost
minutes; `bands` cost 108 minutes) means the loop needs **short
evaluations first** — screen at ~3k ticks, promote survivors to 20k-tick
confirmation — a two-stage evaluation that is itself just two sweeps per
generation. (3k was long enough to rank chains/bands/sieve sensibly in
today's spot checks; the confirmation stage exists precisely because the
screen can mis-rank slow developers.)

## Circuit breakers (driver-level, never sim changes)

- **Wall-time cap per run** (config, e.g. 10× the corpus median):
  runaway-bonding mutants (finding 5) must cost a bounded slice of the
  generation budget. A capped run records `fitness = 0` with a
  `timed_out` mark — selection treats it as dead, ancestry keeps it.
- **Bond-count sanity mark:** a run ending with mean degree > ~50 is
  flagged `condensed` in its record; with fitness (B) it scores low
  anyway, but the mark makes leaderboards honest at a glance.

## Ancestry (reproducibility — same spirit as branch records)

Every individual gets a RON sidecar: parent id, the operator applied,
the exact field path + old/new values, the search RNG seed + draw index,
and the child's config/pack file paths (which are committed artifacts of
a finished search, like sweep records). Replaying an individual =
`genesis score` on its files; replaying a whole search = same seed, same
spec. The chain lives above the engine — precedent: BranchRecord
(Q-2026-07-10-A).

## What this deliberately does not do

- No distributed execution (one box, sequential — ROADMAP non-goal).
- No engine or Observer changes to chase scores: if search keeps finding
  that fitness needs a signal the Observer doesn't measure (e.g.
  quantity-distribution order for bond-free regimes, finding 2), that is
  a QUESTIONS.md entry for a new metric family, not a quiet extension.
- No crossover in v1 — single-parent mutation keeps ancestry a chain,
  not a DAG; add it only if plateau evidence demands.

## Landing order

1. `SearchSpec` + mutation operators + ancestry records, unit-tested
   (validation round-trips, operator bounds, deterministic given seed).
2. Generation loop over the in-process driver + two-stage evaluation.
3. First real search on the sieve/chains/bands neighborhood; findings doc
   `docs/research/sweeps/<date>-search-01.md`; ratify the fitness choice
   in the decisions log with that evidence.
