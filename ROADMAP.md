# PROJECT GENESIS — ROADMAP

Companion to Prompts/MASTER_PROMPT.md (the constitution). This file is the plan and is expected to change; the constitution is not. Phases are sequential but later phases may begin once the previous phase's exit criteria pass.

## Decisions log

Settled — do not re-litigate without cause:

- **Determinism scope:** same platform + same build. Cross-platform bit-exactness is a non-goal.
- **Space:** 2D torus — continuous f32 positions, both axes wrap, no edges.
- **Physics vs interactions (hybrid):** Physics = hardcoded generic continuous operators (motion, forces, diffusion, heat), config-parameterized. Interaction System = discrete data-driven events. Both deterministic, neither Earth-specific.
- **Create/destroy:** interactions may create and destroy particles (split, merge, emit, absorb); matter+energy conserved per event; ids never reused.
- **Time:** fixed dt forever; time warp = more ticks per wall second, never a bigger dt.
- **Save format:** hand-rolled versioned binary (`GENS`, format v1) — no serialization dependency in the format.
- **Information semantics (2026-07-05):** condition-gate + lossy copy. Rule conditions can read it; interactions can copy it between particles at an energy cost with configurable noise; not conserved (creatable by paying energy, lost by decay). Self-replication becomes possible, never built-in.
- **Rule authoring (2026-07-05):** RON data schema — declarative conditions and a fixed action vocabulary, compiled and validated at load, whole pack hashed into replay identity. No scripting in the hot loop.
- **Bond storage (2026-07-05):** SoA edge list keyed by stable ids (a < b, canonically sorted) as save/hash truth + per-tick CSR adjacency mirror for iteration + lookup-only id→index hash map. See docs/research/bond-storage.md.
- **Asteroids / external material (2026-07-06):** an asteroid impact is a replay-recorded event delivering momentum + energy shock + a particle payload. The payload's "material" is specified as quantity ranges (matter/energy/information distributions) — a region of quantity space, never a named substance. Named materials ("minerals") stay banned below the Observer layer; UI/Observer may label payload profiles. **Parked, decide at Phase 4 design review:** a generic `species` axis on particles (per-pair-species kernel table, Particle-Life-style) — powerful but touches physics, rules, saves, and hashing everywhere; must be its own deliberate amendment, never smuggled in via asteroids.
- **Phase 4 ordering (2026-07-06):** adaptive-detail groundwork (chunks ticking at reduced rates, deferred from Phase 2) is the FIRST Phase 4 work item, validated with a ~10M-particle baseline before environment features land. Planet-level *zoom* is a Phase 6 rendering concern and gets cheaper zoomed out (LOD aggregates); the Phase 4 perf cliff is population scale, not the camera.
- **Adaptive detail exactness (2026-07-06, answers Q-2026-07-06-A: Option A):** approximate LOD with the detail policy part of replay identity. Implementation plan drafted in docs/research/adaptive-detail.md (freeze-based both-active gating, per-tick stateless classification, staged landing order; forks marked there are ratified into this log on landing). Chunks are classified by a state-keyed activity metric; cold chunks run every k-th tick. The policy (metric, thresholds, rate ladder) is configuration, hashed into replay identity like physics params — same seed + config + policy = bit-identical replay; a different policy is a different universe. Hard invariants: (1) the policy is configuration, never derived from machine load, thread count, or wall clock; (2) matter/energy conservation holds exactly at every rate, including across cross-rate chunk boundaries; (3) `genesis verify` grows an LOD mode proving LOD-on runs self-identical across thread counts and save/resume, and the ~10M baseline lands in BASELINES.md before any environment feature starts.
- **Adaptive-detail landing forks (2026-07-07, ratifies the forks in docs/research/adaptive-detail.md; Q-2026-07-07-A):** the groundwork landed in the staged order from that doc. Three forks it left to the implementer are now settled:
  - **Classification: per-tick, stateless (no hysteresis).** The activity mask is a pure function of `(state, policy, tick)` — a per-chunk `max` of speed² picks a tick stride from the ladder, and a particle is active iff its chunk's stride divides the tick. No LOD state is saved, so save/resume is bit-identical for free and thread count cannot change a bit. Possible rate *thrash* at a threshold boundary is a performance-only concern; hysteresis (a saved previous-rate array) is a later refinement if profiling demands it.
  - **Chunk geometry: `chunk_cells × chunk_cells` blocks of grid cells, partial edge chunks allowed.** Chunks are pure `GridGeom` geometry (`chunk_of`/`chunk_count`), never wrap and never own state, so a partial chunk at the torus seam is fine. `chunk_cells` and the ladder are `LodPolicy` config, disabled by default.
  - **Replay identity: hash the policy *only when enabled*; save format v8 writes it unconditionally.** This **diverges from the draft doc's hash-always recommendation** (the doc named this the "clean fallback"). Rationale, ordered by our priorities: a disabled policy produces a byte-identical simulation, so it must produce the identical hash — otherwise two configs that never diverge would get different replay identities (a correctness/determinism defect). Consequences: every existing LOD-off run (physics-only, actual/chains/sandbox packs, all recorded baselines) keeps its exact identity with no churn; an enabled policy is a distinct universe and hashes distinctly, even an all-hot ladder that yields identical particle state. The v8 container always stores the policy so an enabled run resumes into the same universe; v7 saves are not migrated (a Phase 7 concern; no v7 saves are committed). The gate is both-active: pairwise forces, bond springs, and interaction events apply only between two active particles, and integration/decay only to active particles — so conservation of matter/energy holds exactly at every rate (a frozen particle never participates; no one-sided transfer), while trajectory accuracy is the approximation the decision buys.
- **Environment fields (2026-07-08, Q-2026-07-08-A):** planet-scale environment is a config-declared list of **generic indexed scalar fields** on their **own coarse torus grid** (`EnvSpec: cols × rows`, SoA values, nearest-cell sampling — deliberately decoupled from both the interaction grid and LOD chunks, so no tuning knob shapes the universe). Field *names* are authoring documentation only — never read by the engine, never hashed, never saved (labels live above the engine, per the asteroid decision). Replay identity follows the v8/LOD precedent: **zero fields contribute nothing to the hash** (every existing run keeps its identity); non-empty fields hash grid dims + all cell values. Fields are state — saves store current cell values (format v9, written unconditionally); init specs (`Uniform`, `GradientX/Y`) are consumed at creation like initial particle ranges and are not identity. Rules gate on fields via `env_cond` (per-field closed bounds) evaluated **at the initiator's env cell**, one sample per candidate ordering (format v10 — compiled rules become variable-length). Field dynamics, physics coupling, and player edits are separate, later decisions. Full rationale: docs/research/environment-fields.md.
- **Player actions (2026-07-08, Q-2026-07-08-B):** a player action is a tick-stamped data record; a headless *action script* is a RON list of them, and the Phase 6 UI will emit the identical records — one representation for scripted, recorded, and live play. v1 vocabulary is env-field edits only (`FieldSet`/`FieldAdd` over an axis-aligned world-coordinate rect; a seam-wrapping region is two rects); rotation/tectonics/asteroids extend the enum when their systems land, and **time warp is never an action** (it cannot affect state). Scheduling: actions apply at the start of their stamped tick in stable script order; past-stamped actions are rejected at assembly. Replay identity: **applied actions are already state** (their effect is the env cells they wrote, hashed since v9) — only the **pending queue hashes**, and only when non-empty, so action-free runs keep their exact identity (v8/v9 precedent). Save format v11 stores the pending queue so a mid-script save resumes into the identical future. `genesis verify --actions` is the executable form of the phase exit criterion. Full rationale: docs/research/player-actions.md.
- **Field dynamics (2026-07-08, Q-2026-07-08-C):** two generic per-field operators, both default-off: **diffusion** (explicit 4-neighbor torus Laplacian in cell units, `diffusion * dt ≤ 0.25` validated, conserves the field total) and **relax** (exponential approach to a rest value — the "climate" a field returns to after edits). The env step runs after the action drain, before the particle step, single-threaded (the env grid is tiny). Dynamics params hash into replay identity only when some field has a non-zero rate; save format v12 stores them unconditionally. Sources are cut from v1 — `FieldAdd` actions + `relax_to` cover authoring until something demands them. Details in docs/research/environment-fields.md §F5a.
- **UI direction (2026-07-08, owner):** WorldBox-style top-down pixel god-sandbox is the confirmed genre shape for Phase 6, with the constitutional differences (environment-only tools, no labels below the Observer, timeline branching). **Observer annotations are panel-only** — never drawn on world objects; selecting a hypothesis may move the camera but renders nothing on the structure. Design parked in docs/design/ui.md (companion to visuals.md).
- **Observer hypotheses v1 + timeline (2026-07-08, Q-2026-07-08-E, ratifies the F5/F6 details in docs/research/observer-design.md):** exactly two confidence-scored hypotheses, evaluated deterministically over the metrics window, positives-only (absence = "nothing to report", never "refuted"): **possibly self-maintaining** (persistence ≥ `self_maintaining_age` with per-sample stability ≥ threshold across the window; confidence = age ramp `min(1, persistence/2·age_min)` × min window stability) and **possibly growing** (full-window presence, non-decreasing size, net increase; confidence = strict steps / (window−1)). Life/intelligence/civilization labels stay unshipped until metrics could honestly move them. The timeline records (tick, stats, metrics, hypotheses) per sample — member lists deliberately excluded (stable observer ids are the reference) — dumped as RON via `genesis run --timeline`; never saved, never hashed.
- **Observer metrics v1 (2026-07-08, Q-2026-07-08-D, ratifies the F4 fork in docs/research/observer-design.md):** per-structure metrics are pure graph/quantity facts computed per sample: persistence = age in samples; stability = Jaccard similarity of consecutive memberships (`1 − churn`, newborn = 1.0); **complexity = `ln(size) + degree-entropy + ln(1 + mean_degree)`** — the doc's literal "size + degree entropy" would tie a ring with an equal-size dense blob (both zero entropy), so a connectivity term is added; information retention = current member information total (lifetime trend is timeline work, F6). Observer config and metrics are never replay identity — the Observer cannot affect the simulation. Adaptation metric deferred until timeline recording exists.
- **Asteroid impact action (2026-07-09, Q-2026-07-09-A, implements the 2026-07-06 asteroid decision):** `ActionKind::Impact` joins the player-action vocabulary (save format v13). An impact at a world point is (a) a **momentum shock**: particles within `radius` (torus metric) take a radially-outward velocity impulse with linear falloff, divided by their matter (`dv = impulse·(1−d/r)/m`; a particle exactly at the point takes none); (b) an **energy shock**: the declared total splits across in-radius particles proportionally to falloff weight — deposited in full when anyone is in radius, entirely lost when nobody is; and (c) a **particle payload**: `count` particles drawn uniformly from declared matter/energy/information/speed ranges (quantity space, never a named substance — the 2026-07-06 rule), spawned on a spread disc and ejected radially, ids from the normal sequence. Payload RNG is the order-free stream `derive(stream_seed, [IMPACT_TAG, tick, queue_index])`, so same-tick impacts draw independently and script/resume/UI deliveries are identical. Replay identity follows Q-2026-07-08-B exactly: pending impacts hash (every parameter), applied impacts are already state. **Injection is deliberate**: matter/energy arrive from outside the world (external material, 2026-07-06) and tests pin the injection to exactly the declared payload + shock. Shipped example: `scripts/bombardment.ron`.
- **Renderer snapshot mechanism (2026-07-09, Q-2026-07-09-B, answers open design question 4):** v1 renderer is **lockstep with an extraction seam**: one thread owns the `Simulation`, ticks it per frame (0 when paused, budget-bounded k under warp, wall clock deciding only *how many whole ticks* — never entering the sim), then an extraction pass produces a `RenderFrame` of plain data (T0/T1 sprite instances, T2/T3 cell aggregates per docs/design/visuals.md) that rendering alone consumes. Because consumers never see sim references, promoting extraction onto a dedicated sim thread with a double buffer later changes no consumer code — the seam is the point. Copy-on-write rejected (taxes the SoA layout for nothing the seam doesn't buy). No interpolation in v1; warp UI shows target vs achieved ticks/s. New `genesis-render` crate (full Bevy) + windowed bin; `genesis-headless` keeps zero renderer deps; sim crates stay `bevy_ecs`-only. Landing order and testable-on-headless boundaries: docs/research/render-bootstrap.md.
- **Tectonic events (2026-07-10, Q-2026-07-10-C, option A; save format v14):** a tectonic event is `ActionKind::Rift` — an impact whose shock source is a world-coordinate **segment**: particles within `radius` of the segment (torus metric; the authored segment vector is taken literally, so a segment may cross the seam) take a perpendicular outward impulse with linear falloff, the declared energy splits by falloff weight (id-order sum, the impact determinism rule), and the payload spawns like a point impact at a uniformly drawn segment point (upwelling; stream tag distinct from impacts). A degenerate segment is bitwise a point impact. Every impact invariant carries over unchanged: injection is exactly the declared payload + shock, pending rifts hash (every parameter), applied ones are state. Adopted per the standing guidance — a pure generalization of the shipped impact system; rotation (Q-2026-07-10-B) and magnetic field (Q-2026-07-10-D) stay parked. Shipped example: `scripts/rift.ron`.
- **Timeline branch records (2026-07-10, Q-2026-07-10-A, the logic core of the Phase 6 branching deliverable):** a branch is an ordinary save file plus a **RON sidecar record** (`BranchRecord`, format 1, in genesis-persist under its replay-recording charter): `parent` — the parent's save path, state hash, and fork tick, `None` for a root — plus this branch's own player-action log (append-only, apply order; the UI appends every record it emits, a scripted run's log is its script). Ancestry lives **above the engine**: the binary save format is untouched (no v14), the engine never reads a record, and nothing about ancestry enters replay identity — the labels-above-the-engine precedent (asteroids, 2026-07-06) and the Observer precedent, applied to bookkeeping. Forking copies the save (the child inherits the exact state, pending queue included) and starts an empty log; children of one parent share ancestry by referencing the same chain. `genesis branch --from parent.gens --to child.gens` is the headless operation; the Phase 6 UI performs the identical steps. Packaging a chain into one shareable replay file is Phase 7's deliverable and may revisit containers without touching identity.
- **Planet rotation (2026-07-12, owner, answers Q-2026-07-10-B: option C staged — frame spin now, insolation later):** rotation on a 2D torus is **frame spin**: a physics param `spin` (f32, default 0, either sign) — the angular velocity Ω of the world frame — applying the Coriolis acceleration `a = 2Ω·perp(v)` to every active particle. This is the geophysics f-plane precedent (the standard way rotation enters flat 2D domains; the centrifugal term has no center on a torus and drops out, exactly as the f-plane drops it). Owner-reviewed literature findings in docs/research/rotation.md. The conservation objection that parked this fork dissolves: the Coriolis force does no work, so **energy conservation is exact**; total momentum is not lost but **rotates at constant magnitude** (`dP/dt = 2Ω·perp(P)`, the Coriolis–Lorentz analogy) — |P| stays a checkable invariant. Numerics: applied as an **exact velocity rotation** by `−2·spin·dt` after the force kick (the plasma Boris-pusher precedent) — an explicit Euler force term would inject energy every tick; the rotation is speed-preserving and unconditionally stable. Replay identity: `spin` hashes **only when non-zero** (v8 precedent — a spin-0 world is byte-identical to a pre-spin world); save format v15 writes it unconditionally. Player verb `SpinSet { spin }` sets the param at its stamped tick through the one action path; pending SpinSet hashes, applied SpinSet is state (the param). Emergence rationale: rotation is a first-class 2D pattern generator (inverse-cascade coherent vortices; chiral-active-matter vortex crystals). Deliberately not built: spatially varying spin (β-plane zonal jets — a possible later extension) and insolation cycling (option B — a field-dynamics oscillator, its own future decision, not a rotation feature).
- **Run-score record (2026-07-13, Q-2026-07-13-A, implements the first Phase 6.5 deliverable):** a scored run emits one RON `ScoreRecord`: an identity stamp (seed, ticks, sample cadence, **final state hash** — the replay fingerprint; config/pack/script *paths* ride along as documentation only, per the labels-above-the-engine precedent) plus a flat `RunScore` aggregating the observer timeline — final and peak values for structure count / largest size / lifetime / complexity / structure-held information, distinct-structure counts and peak confidences per hypothesis kind, and one headline scalar for the phase exit criterion: **max over every (structure, sample) of persistence × complexity** (long-lived *and* structured beats either alone; complexity is strictly positive for any ≥2-member structure, so the product never rewards degenerate cases). Both types live in `genesis-observer` (`score` module) so the sweep driver consumes the same definitions; the CLI (`genesis score --ticks N --every M [--out f]`) is a thin wrapper. Scores are Observer output — never replay identity, never saved into `.gens`, and observer config is not stamped into the record (it changes what is *reported*, never what *happened*; the cadence is stamped because it selects which ticks the timeline aggregates). Peaks are taken alongside finals so a regime that flourishes and collapses before the last tick still registers — end-state-only scoring would systematically undervalue exactly the transient dynamics the experiment loop hunts for.
- **Sweep driver (2026-07-13, Q-2026-07-13-B, implements the second Phase 6.5 deliverable):** a sweep is a RON spec — sweep-level defaults (`ticks`, `every`, one observer config so all scores in a batch are comparable) plus an **explicit run list** (name + config/pack/script paths, per-run tick/cadence overrides); grid generation is deferred to the search deliverable, which will *emit* explicit lists, keeping one spec format. Runs execute **sequentially** through the exact `score_run` path `genesis score` uses (one code path, the one-representation precedent of Q-2026-07-08-B applied to tooling); outputs are `<name>.score.ron` per run plus `table.md` sorted by the headline score with name as tiebreak — batch order deliberately cannot reach any record or the table (per-run determinism is the engine's guarantee; order-independence of the report is enforced by sorting and tested). Run names are validated as filenames (`[A-Za-z0-9._-]`, unique). The shipped corpus spec `sweeps/shipped-packs.ron` — every pack with its canonical pairing plus a physics-only control — is the baseline the exit criterion measures discovered regimes against.
- **Search generation loop (2026-07-13, Q-2026-07-13-C, implements search step 2 and ratifies the design doc's forks):** `genesis search --spec S --out DIR` runs the evolutionary loop: seeds screened at `screen_ticks`, truncation selection (top-`survivors` of *everything evaluated so far* — implicit elitism; total order by fitness `total_cmp` descending then id, so no tie is left to chance), one mutation per child via the step-1 operators, and one end-of-search confirmation of the all-time top-`confirm_top` at `confirm_ticks`. **Fitness v1 is ratified as shipped** (design fork B, evidence: the baseline sweep): `ln(1+structures_final) × ln(1+lifetime_peak) × (1+ln(1+ln(1+information_final)))` — saturating terms so neither one immortal blob nor a spray of fragments can win; the raw headline scalar is always *reported*, never *climbed* (the exit criterion is judged on records, not on fitness). Two deliberate divergences from the design doc, per the divergence-is-written rule: (a) the **circuit breaker is a bond-count cap, not a wall-time cap** — evaluations stop at the first observer sample whose bond count exceeds `bond_cap`, scoring what happened with fitness 0 (record + ancestry kept, `capped` mark). A wall-clock cap would make the search trajectory machine-dependent, contradicting the reproducibility the same doc demands; bond count is the wall-time driver (baseline finding 5), simulated state, and fires identically everywhere. (b) **confirmation runs once at the end over the all-time top-k, not per generation** — a bonded 20k-tick run costs minutes to hours (the sieve score run spanned shifts), so per-generation confirmation would dwarf the search; the cost is that selection trusts the short screen, which can mis-rank slow developers (recorded as a known limit, revisit on plateau evidence). Condensation (mean bond degree > 50) is *marked* on leaderboards, never penalized — fitness already declines to reward it. A finished search directory is a self-contained, committable artifact (per-individual config/pack/ancestry/score with search-relative stamped paths, `leaderboard.md`, `summary.ron`); the whole search is a pure function of (spec, build) — re-running reproduces every file byte-for-byte (tested) — and every child is independently reproducible by `genesis mutate` on its parent's committed files, because children mutate from the on-disk parent artifact, not the in-memory copy.
- **Search instrument v1.1 (2026-07-14, Q-2026-07-14-A, applies search-01 findings 1 and 3):** two spec-level extensions to `genesis search`, both defaulting to search-01 behavior so existing specs mean what they meant. **`mutations_per_child`** (default 1): k mutations drawn sequentially from the child's one derivation stream — bitwise the same chain `genesis mutate --steps k` draws, so any child stays hand-reproducible in one command (tested: replaying the recorded op chain from the parent's on-disk artifact reproduces the child's files). Motivated by search-01 finding 1: a homeostatic regime is a plateau under single σ=0.3 mutations; bolder compound steps are the cheapest escape that changes no metric and no fitness. **`confirm_bond_cap`** (falls back to `bond_cap` when omitted): the confirmation stage's own circuit breaker. The cap is a per-evaluation *cost* bound and bonds grow with the horizon, so one cap for both stages spuriously kills every longer confirmation run — search-01 measured exactly that (both 6k confirms tripped the 3k-sized cap). Ancestry sidecars now record the full operator chain (`ops`, application order); the committed search-01 sidecars (single `op`) still load via a deserialization shim, and `genesis mutate` records every step instead of only the last. Adopted per the standing guidance: instrument refinements prescribed by committed findings, no simulation code touched, no replay-identity surface — same build + same spec still reproduces a search byte-for-byte.
- **Search instrument v1.2 — late-bond-growth field; detonation mark rejected by measurement (2026-07-17, Q-2026-07-17-A):** every `RunScore` now records `bonds_growth_late` — bonds at the last observer sample over bonds at the two-thirds sample (denominator floored at 1; neutral 1.0 for empty timelines and, via serde default, for every record written before the field existed — search-01..04 artifacts load unchanged). Motivation: searches 02–04 each saw the screen's #1 detonate in confirmation, and search-04's findings parked a "mid-screen bond-growth-rate mark" as the cheap instrument answer. The mark half was **measured and rejected**: at the 3 k screen the two search-04 detonators show final-third growth 1.18 / 1.35 against the honest champion's 1.17 — indistinguishable — so the detonation is scheduled wholly past the horizon and a threshold flag would separate nothing (a mark that cannot discriminate is worse than none; the same measurement session reproduced the committed screen state hashes across the code change, confirming the field cannot touch sim bits). The field ships report-only — it quantifies detonation intensity in capped confirmation records and lets any future horizon re-test the hypothesis from records alone; no leaderboard flag, no summary field, nothing enters fitness, selection, or replay identity. Adopted per the standing guidance (tooling-only, additive, precedent Q-2026-07-14-A); the negative result is recorded here precisely so no future shift re-invents the mark without new evidence.
- **Bounded-degree headline measurement — report-only (2026-07-17, Q-2026-07-17-B; ships the measurement half of Q-2026-07-15-A option 2 without deciding it):** every `RunScore` now records `persistence_complexity_bounded` — the same max persistence × complexity, taken only over (structure, sample) rows whose per-structure mean bond degree respects the condensation mark (≤ 50). The mark constant moved to `genesis-observer::score::CONDENSED_MEAN_DEGREE` so the search's leaderboard flag and this field can never disagree on what "condensed" means. Type is `Option<f64>`, serde-defaulted to `None`: a maximum has no neutral sentinel, so a record written before the field existed must stay distinguishable from a measured "nothing qualified" (`Some(0.0)`) — every committed record loads unchanged. Sweep tables grow a `bounded` column ("—" for legacy records). The field is Observer output — never fitness, never selection, never replay identity (state hashes of every re-scored run reproduce the committed records bit-for-bit, 3k corpus + 20k subset, docs/research/sweeps/2026-07-17-bounded-headline.md) — and the exit criterion **remains scored on the raw scalar**: whether it moves to the bounded scalar is exactly the parked owner decision (Q-2026-07-15-A), which this field exists to inform with measured numbers instead of the anatomy doc's analytic ceilings. Adopted per the standing guidance (tooling-only, additive; precedents Q-2026-07-17-A and Q-2026-07-14-A).
- **Information overflow cap (2026-07-06, answers Q-2026-07-06-B: Option A, information-only):** clamp information into `[0, information_max]` at interaction commit. `information_max` is a physics param in replay identity (save format bump); default 1e30 — far above any meaningful signal, far below f32 overflow in transfer arithmetic. Matter and energy are conserved by construction and stay uncapped. Amplifying packs saturate instead of detonating to NaN. **Shipped 2026-07-06 (save format v7):** `information_max` is a validated `PhysicsParams` field (finite, > 0, default 1e30), hashed into replay identity; the clamp is applied to both touched particles at interaction commit, and to emitted children at birth. Verified deterministic (fresh/resume/thread-count) on physics-only, sandbox, and hoarders packs.

## Phase 1 — Foundation (done, v0.1.0)

Defined in MASTER_PROMPT.md. Workspace, headless simulation, particle type, deterministic RNG, fixed-timestep loop, ECS, config, logging, save/load framework, tests.

Proposed workspace layout (one crate per layer, dependencies flow downward only):

```
genesis/
  crates/
    genesis-core/      # particle type, fundamental quantities, deterministic RNG, state hash
    genesis-sim/       # simulation loop, scheduling, chunk framework, ECS integration
    genesis-config/    # simulation + rule configuration, validation
    genesis-persist/   # versioned binary save/load, replay recording
    genesis-headless/  # CLI binary: run, benchmark, replay-verify
  Cargo.toml           # workspace root
  rust-toolchain.toml
```

Renderer, observer, and AI crates are added in their own phases — not scaffolded empty now.

## Phase 2 — Physics and Space (done, v0.2.0)

Goal: particles move, collide, and exchange matter/energy under conservation laws, in parallel, deterministically.

Shipped: SoA particle store in canonical (cell, id) order; uniform torus grid; generic short-range kernel (Particle-Life-shaped: repulsion core + attraction band); semi-implicit Euler; rayon chunk-parallel force/integrate passes with thread-count-invariant hashing (proven at 1M particles, see BASELINES.md); matter conserved exactly, momentum/energy within tolerance; save format v2 (physics params in replay identity). Adaptive-detail groundwork deferred to when Phase 3 makes cells meaningfully idle.

Deliverables:

- Spatial partitioning: chunk grid, neighbor queries, particle migration between chunks. Torus wrapping in all position math.
- Motion integration (fixed timestep), collision/force resolution.
- Conservation accounting: total matter and energy tracked per tick; leaks fail tests.
- Multithreaded chunk simulation using the two-phase collect-then-commit rule from the constitution.
- Profiling harness and benchmark suite (criterion or similar) with tracked baselines.
- Adaptive simulation detail groundwork: chunks can run at reduced rates without breaking determinism.

Exit criteria: millions of interacting particles; state hash identical between single-threaded and multithreaded runs of the same seed; conservation tests pass over long runs.

Non-goals: chemistry, bonds, any interaction beyond physical forces.

## Phase 3 — Interaction System and Chemistry (done, v0.3.0)

Goal: the data-driven interaction engine — the heart of emergence.

Shipped: the full interaction action vocabulary — quantity transfers; bonds (canonical id-keyed edge list + per-tick CSR mirror per docs/research/bond-storage.md, harmonic springs, rule-driven create/break); lossy info_copy + physics.information_decay (information deliberately non-conserved); particle emit/absorb (split/merge — emissions append mid-commit, absorptions mark-dead + compact, matter+energy conserved per event, ids never reused; save format v6). All on the RON rule-pack layer with per-pair derived RNG streams. Two-packs-one-engine deliverable shipped: packs/actual.ron (conservation-respecting) vs packs/sandbox.ron (amplifying), same engine, both verified deterministic from the same seed. Exit-criteria review passed 2026-07-06 (docs/research/phase3-exit-review.md): chains.ron produces ~160 persistent multi-particle structures over 20k ticks (oldest keeps identity 19.5k ticks), all packs verify deterministic (fresh/resume/thread-count), conservation exact; f32 info overflow under amplifying content raised as QUESTIONS.md Q-2026-07-06-B. Read-only structure diagnostics live in `genesis run --report N`.

Deliverables:

- Interaction rule format (condition → action → probability → costs → transfers) as data files, validated at load, hashed into the replay identity (a rule change is a version change).
- Bonds between particles; compound structures as pure consequences of bonds, never as objects the engine names.
- Matter/energy/information transfers through interactions.
- Particle creation and destruction through interactions (split, merge, emit, absorb), conserved per event; ids never reused.
- First concrete semantics for the information quantity (open design question — see below).
- Actual Physics and Sandbox Physics as two rule packs on the same engine, not two code paths.

Exit criteria: a nontrivial rule pack produces persistent multi-particle structures nobody explicitly coded; determinism and conservation still hold.

Non-goals: anything that names biology; environment simulation.

## Phase 4 — Environment and Planet

Goal: the world the player actually manipulates.

Progress: **adaptive-detail (LOD) groundwork shipped 2026-07-07** — the first
Phase 4 work item (decisions log, 2026-07-06). Chunks (`chunk_cells × chunk_cells`
blocks of grid cells) are classified each tick by a stateless activity metric;
cold chunks run at a reduced rate via a config ladder (`LodPolicy`, disabled by
default). The both-active gate keeps matter/energy conservation exact at every
rate (proven by a cross-rate emit/absorb test); the policy is replay identity
only when enabled (save format v8), so every existing LOD-off run keeps its
identity. `genesis verify --config configs/lod.ron` proves LOD-on self-identical
across thread counts and save/resume; the ~10M baseline is in BASELINES.md.
Forks ratified in the decisions log (Q-2026-07-07-A). The recorded follow-up
(the per-tick `canonicalize` sort was unskipped and bounded the speedup)
**landed 2026-07-09**: the sort is now incremental — only cell-changers are
re-placed — so it scales with motion, not population; bit-identical layout by
construction, all 2026-07-07 baseline hashes reproduced (BASELINES.md).

**Environment fields shipped 2026-07-08** (Q-2026-07-08-A, save formats v9/v10):
generic indexed scalar fields on their own coarse torus grid, built from init
specs (`Uniform`, `GradientX/Y`); rules gate on them via `env_cond` at the
initiator's env cell. Zero fields / env-free rules keep every pre-env replay
identity bit-for-bit (verified against prior builds). Content:
`configs/env-gradient.ron` + `packs/bands.ron`; a sim test proves bonded
structures concentrate where the environment allows.

**Player action stream shipped 2026-07-08** (Q-2026-07-08-B, save format v11):
tick-stamped `FieldSet`/`FieldAdd` records over world-coordinate regions,
drained at the start of their stamped tick; pending actions hash into replay
identity, applied actions are already state. `genesis run/verify --actions`
runs scripted experiments headless; `scripts/terraform-west.ron` on the
gradient world is the exit criterion executable — verified DETERMINISTIC over
3000 ticks across fresh/resume/thread-count, with a test showing the scripted
edit redirects where structures emerge. **Both exit criteria now pass.**
**Field dynamics shipped 2026-07-08** (Q-2026-07-08-C, save format v12):
per-field diffusion (torus Laplacian, conserves the field total) and
relaxation toward a rest value, both default-off; params in replay identity
only when active. The full stack — dynamic field + env-gated rules + action
script — verifies DETERMINISTIC over 3000 ticks.

**Asteroid impacts shipped 2026-07-09** (Q-2026-07-09-A, save format v13):
`ActionKind::Impact` — momentum + energy shock with linear falloff plus a
particle payload drawn from quantity ranges, replay-recorded like every
player action; pending impacts hash, applied ones are state. Injection is
exactly the declared payload + shock (tested); `scripts/bombardment.ron`
verifies DETERMINISTIC across fresh/resume/thread-count. **Hardened
2026-07-09 (night review):** the shock's falloff-weight sum is taken in
particle-id order — the drain-time store layout differs between an
uninterrupted run and a fresh resume, so an impact stamped for the save
tick used to diverge bitwise; regression-tested. Rim membership is now
positive falloff weight, closing a ULP edge that could silently drop the
whole energy deposit.

**Tectonic events shipped 2026-07-10** (Q-2026-07-10-C, save format v14):
`ActionKind::Rift` — an impact-shaped shock off a world-coordinate
segment (perpendicular impulse, falloff-weighted energy, payload
upwelling along the line), every impact determinism invariant intact;
`scripts/rift.ron` verifies DETERMINISTIC across fresh/resume/
thread-count. **Hardened 2026-07-10 (night review):** the shock
projection used the torus-*folded* offset from the segment start, so
once segment length + radius exceeded half the world the far end-cap
was silently skipped (rift.ron authors exactly a half-world segment);
the projection now takes the true torus minimum over adjacent world
copies — safe segments keep their exact bits, and a segment spanning
more than one world period per axis is rejected at both intake paths.
Programmatically built scripts are now structurally validated at
assembly like live-queued actions (one acceptance boundary), and the
full-stack scenario pairing is additionally pinned across thread
counts and a mid-script save/resume.

Rotation verb settled by the owner 2026-07-12 (Q-2026-07-10-B, decisions
log: frame spin + `SpinSet`, save format v15, docs/research/rotation.md).
Remaining Phase 4 work: the magnetic field verb (parked as
Q-2026-07-10-D — do nothing until radiation exists), and chunk
streaming — scoped and **deliberately deferred** until an out-of-memory
scale target exists (docs/research/chunk-streaming.md). Phase 5 may
begin: this phase's exit criteria pass.

Deliverables:

- Planet-scale environment fields: temperature, pressure, gravity, radiation, atmosphere composition — sampled by chunks, influencing interaction conditions.
- Player environment tools (the only player verbs): adjust fields, rotation, magnetic field, tectonic events, asteroid impacts, time scale.
- Player actions recorded in the replay stream (constitution rule 6 covers them).
- Chunk streaming / persistence so planets exceed memory.

Exit criteria: a player script (headless) can shape an environment and replay it identically; environment gradients visibly shape where structures from Phase 3 emerge.

Non-goals: rendering, UI.

## Phase 5 — Emergent Structures and Observer

Goal: detect what emerged, without ever influencing it.

Progress: design draft in docs/research/observer-design.md (2026-07-08).
**Step 1 landed 2026-07-08**: `genesis-observer` crate (the CLI's analysis
module, promoted) — read-only by construction, with the on/off
replay-compatibility test proving observation changes no simulated bit.
**Step 2 landed 2026-07-08**: configurable overlap threshold + stable
observer-side structure ids (`ObserverConfig`, RON, never replay
identity; ids never reused), exposed via `genesis run --observer`.
**Step 3 (metrics v1) landed 2026-07-08** (Q-2026-07-08-D): persistence,
stability (Jaccard of consecutive memberships), complexity
(`ln(size) + degree-entropy + ln(1 + mean_degree)`), information
retention — per structure per sample, deterministic.
**Step 4 (hypotheses v1 + timeline) landed 2026-07-08**
(Q-2026-07-08-E): *possibly self-maintaining* and *possibly growing*,
confidence-scored, positives-only; timeline records (tick, stats,
metrics, hypotheses) per sample, RON-dumpable via
`genesis run --timeline`. **Exit-criteria review passed 2026-07-08**
(docs/research/phase5-exit-review.md): hypotheses track what the eye
sees in chains.ron and bands.ron runs; observer on/off produces the
identical state hash at both library and CLI level. Phase 6 may begin.
Deferred (recorded in the design doc): adaptation metric, richer
hypotheses, zero-copy snapshots (open design question 4 — a Phase 6
problem).

Deliverables:

- Observer as a separate crate that consumes read-only snapshots; physically cannot mutate simulation state.
- Structure detection and tracking over time (clusters, cycles, self-maintaining patterns).
- Metrics: complexity, entropy, information density, persistence.
- Hypothesis system: "possibly self-replicating", "possibly homeostatic" — confidence-scored, never absolute.
- History/timeline recording for later narration.

Exit criteria: Observer flags structures in Phase 3/4 outputs that match what developers see by eye; simulation state hash provably unaffected by Observer presence.

Non-goals: natural-language output; rendering.

## Phase 6 — Rendering and Player Experience

Goal: make it watchable and playable.

Progress: **snapshot mechanism settled 2026-07-09** (Q-2026-07-09-B,
answers open design question 4) — lockstep with a `RenderFrame` extraction
seam; full landing order in docs/research/render-bootstrap.md. Design
groundwork already parked: docs/design/visuals.md (quantity→visual mapping,
LOD tiers T0–T3), docs/design/ui.md (WorldBox-shaped chrome, owner-settled
2026-07-08). **Step 1 (extraction core) landed 2026-07-09**: `genesis-render`
crate (deliberately Bevy-free until step 2) — tier selection by particles
per pixel, T0/T1 camera-space sprite extraction (torus-seam correct by
construction), T0 bond segments (seam bonds render as the short segment),
T2/T3 cell aggregation, RON `VisualMapping` loader (never replay identity).
Read-only at the type level; fully unit-tested headless. **Hardened
2026-07-09 (night review):** views wider than the world tile wrapped
instance copies (bond copies land exactly on sprite copies), bonds cull
by segment box instead of vanishing while crossing a zoomed-in view,
mappings validate at load, hue stays in [0,1) at the wrap edge.
**Step 3 logic half landed 2026-07-09**: `raster` module — RON palette
ramps (`palettes/`: default, colorblind-safe, debug-gray) and cell
aggregates → RGBA8 with torus-wrapped world-rect sampling + 4×4 Bayer
ordered dithering, pure and byte-deterministic.
**Step 4 logic half completed 2026-07-10** (pacer landed 2026-07-09):
`brush` module + `Simulation::queue_action` — a brush stamp becomes
1/2/4 seam-wrapped rects → ordinary `PlayerAction` records fed through
the one scripted-action path (Q-2026-07-08-B), with validated live
enqueue on the sim (Result, never panic; queue-mid-run bit-identical to
the same action scripted, survives save/resume).
**Step 5 logic half landed 2026-07-10**: `inspect` module —
screen→world transform, torus-metric particle picking, structure
member lookup, and circular-mean structure focus (a seam-straddling
structure centers the camera on the seam).
**Timeline branching logic core landed 2026-07-10** (Q-2026-07-10-A):
`BranchRecord` RON sidecar in genesis-persist (ancestry above the
engine, never replay identity) + `genesis branch` — fork a save into an
independent branch whose untouched continuation is bit-identical to the
parent's and whose own actions diverge it alone. The UI half (fork
button, timeline tree) rides with step 2+. **Hardened 2026-07-10
(night review):** forking now lives in the library as
`BranchRecord::fork_save` (CLI and future UI share it) and refuses to
overwrite an existing child save or record — overwriting silently
destroyed that branch's state and action log, and `--from X --to X`
wrote a cyclic ancestry record.
**Step 2 (Bevy app shell) landed 2026-07-12** on a desktop session with a
display, plus the GPU half of step 3 and the input half of step 4:
`genesis-app` (a bin in genesis-render behind the `app` feature — full
Bevy 0.19 stays out of `cargo test --workspace`; a trimmed feature set
avoids the wayland/gilrs/alsa system-dev-lib builds). Lockstep frame
loop per the Q-2026-07-09-B decision: the app owns the `Simulation`,
`WarpPacer` plans whole ticks from measured frame time, extraction
produces the `RenderFrame` rendering consumes. T0/T1 sprite pool (one
procedural soft-dot texture, mapping-tinted) + gizmo bond lines; T2/T3
heatmaps rasterized by the step 3 logic half and uploaded to a
nearest-sampled texture at 1/4 resolution (the integer-upscale pixel
look); HUD with tick / target-vs-achieved ticks/s / starvation flag /
tier. Input: wheel zoom, WASD/right-drag pan, space pause, 1–4 warp
presets, and the left-drag field brush emitting replay-recorded
`PlayerAction`s through `Simulation::queue_action` (one representation).
`--smoke N` runs the real window N frames and prints tick + state hash —
verified under WSLg (llvmpipe software vulkan) on the default config
(T3 heatmap), `--zoom 40` (T0 sprites + bonds), and
configs/env-gradient.ron. Remaining Phase 6: observer/inspector panels,
save/load/branch UI (step 5 Bevy half), visual polish on a real GPU.
**User-guide plan parked 2026-07-12**: docs/design/guide.md — ten-chapter
`docs/guide/` plan (install → tutorial → app/CLI references → authoring →
actions/replay → saves/forks → Observer → performance), each chapter
play-facing first with a *Technical notes* close. Chapters 1, 5–10 are
writable now against shipped behavior; chapters 2–4 wait on the owner's
Windows GPU test, which also decides whether a menu screen exists (that
verdict reshapes onboarding — draft the tutorial chapter last).
**Writable-today set shipped 2026-07-15**: chapters 1, 5, 6, 7, 8, 9, 10
landed in docs/guide/ with a ToC README, every command executed verbatim
on the writing box (script verifies, validation errors, observer
configs, save/branch flows); root README links the guide. Remaining:
chapters 2–4 on the planned blockers.

Deliverables:

- Bevy renderer as a pure consumer of simulation snapshots (renderer never owns simulation logic).
- Retro pixel art, top-down camera, continuous zoom planet → particle with rendering LOD independent of simulation LOD.
- Environment-tool UI, time controls, Observer overlay (hypotheses, metrics, timelines).
- Save/load/replay UI.
- Timeline branching: fork a run into an independent save + player-action log with shared ancestry metadata.
- User guide (`docs/guide/`): comprehensive, technical, copy-paste-runnable — install through authoring, replay, and Observer (plan: docs/design/guide.md).

Exit criteria: full loop — empty planet, shape environment, run, observe, experiment — with no objectives and no loading screens in the common path.

Non-goals: AI narration.

## Phase 6.5 — The experiment loop (owner-approved 2026-07-12, runs alongside Phase 6)

Goal: turn the telescope. The engine is built; the open risk is that the
shipped physics + rule packs plateau at "blobs, bands, persistent chains" —
open-endedness is the unsolved problem of the field and will not fall out
by accident. This phase does the science: search parameter/rule space for
regimes that score high on Observer metrics, instead of hand-authoring
packs one at a time. Everything here is headless and cloud-verifiable
(PLAYBOOK §5 boundary), so it is the sanctioned night-shift work while the
remaining Phase 6 UI items wait on desktop sessions.

Progress: **run scoring shipped 2026-07-13** (Q-2026-07-13-A) —
`genesis score` runs a config/pack/script for N ticks, samples the observer
on a cadence, and emits one RON `ScoreRecord` (identity stamp + final/peak
aggregates + the exit-criterion scalar `persistence_complexity`); the
`RunScore`/`ScoreRecord` types live in genesis-observer for the sweep
driver to reuse. Verified bit-identical across repeat runs at library and
CLI level. **Sweep driver shipped 2026-07-13** (Q-2026-07-13-B) —
`genesis sweep --spec S --out DIR` runs an explicit RON run list
sequentially through the same `score_run` path, writing per-run records
plus `table.md` sorted by headline score (batch order cannot show
through); `sweeps/shipped-packs.ron` is the shipped-content corpus the
exit criterion measures against. **Screen-horizon corpus gate added
2026-07-14** (`sweeps/shipped-packs-3k.ron` +
docs/research/sweeps/2026-07-14-shipped-packs-3k.md): the same corpus at
the search screen horizon (3k ticks, ~2.5 min total) — the cheap first
gate in front of any 20k champion evaluation; the 20k baseline stays the
exit-criterion authority. Key finding: the horizon reorders the
leaderboard (sandbox tops 3k, the condensers top 20k), so the gate
filters, never verdicts.

Deliverables, in dependency order:

- **Run scoring** (done): a `genesis-headless` mode that runs a config/pack for N
  ticks and emits one machine-readable score record (RON) from Observer
  metrics — structure count/size/lifetime, complexity, information
  retention, hypothesis confidences. Deterministic, seed-stamped.
- **Sweep driver** (done): run a batch of configs/packs (grid or explicit list),
  collect score records, write a comparison table. Sequential is fine;
  determinism per run matters, batch order must not.
- **Search**: mutate the best-scoring configs (parameter jitter, rule
  add/drop within schema) and iterate — a basic evolutionary loop over
  worlds. Mutations logged so any discovered regime is reproducible from
  its ancestry (same spirit as branch records). Design drafted
  2026-07-13 (docs/research/search-design.md): saturating-product
  fitness over the raw headline scalar (the baseline sweep shows the
  scalar alone breeds condensation), schema-bounded mutation operators,
  two-stage evaluation, sidecar ancestry; forks ratified into this log
  on landing. **Step 1 landed 2026-07-13**: mutation operators
  (jitter / drop / duplicate-and-jitter / condition rewire, all
  repair-clamped and re-validated; mutants are pure functions of
  (seed, generation, individual) via order-free stream derivation) +
  RON ancestry sidecars + `genesis mutate` for hand experiments. The
  generation loop (step 2) carries the fitness decision. **Step 2 landed
  2026-07-13** (Q-2026-07-13-C): `genesis search` — truncation selection
  with elitism, one mutation per child, screen-then-confirm evaluation,
  deterministic bond-count circuit breaker; a finished search directory
  is a self-contained artifact (ancestry + records + leaderboard +
  summary), and re-running the spec reproduces it byte-for-byte.
  Fitness v1 ratified in the decisions log. **First real run (search-01)
  2026-07-13** (docs/research/sweeps/2026-07-13-search-01.md): sieve
  lineage displaces chains at once, then plateaus ~1% above the seed —
  the neighborhood is flat at σ=0.3; four instrument lessons recorded
  (screen-horizon lifetime saturation, horizon-aware bond caps,
  BIND-visibility ridge, corpus-horizon cost). **Second run (search-02)
  2026-07-14** (docs/research/sweeps/2026-07-14-search-02.md), the
  controlled comparison under instrument v1.1 — same seeds/horizon,
  bolder steps (σ 0.6 × 3 ops/child): escapes the plateau (+4.5% vs
  +1%) by *leaving* sieve — the champion lineage strips FUEL/SPLIT/SHED
  and multiplies information-carrying BIND variants into an accretive
  imprint-web regime with bounded bond growth; its 6k confirmation
  runs uncapped (the confirm_bond_cap fix working both ways — its
  near-tied sibling detonated 900 ticks past the screen and was
  correctly capped). At the corpus horizon (20k, 489s — affordable,
  unlike sieve-class regimes) the champion scores **2421.60: third
  place, beating sandbox/full-stack/chains but not actual (3631) or
  bands (2712)** — the first discovered regime competitive at 20k. The
  exit criterion stays open; the sharpened question (findings doc §5):
  can a non-condensing regime reach 3600+, or does the scalar
  structurally favor condensation? **Third run (search-03) 2026-07-15**
  (docs/research/sweeps/2026-07-15-search-03.md): sieve vs
  gradient-sieve in one pool under the same instrument — the cline-world
  lineage swept ranks 1–26 and generation 3 *duplicated* the env-gated
  cull under uniform drop pressure; the confirmed champion (g005-i005,
  fitness 88.90 at 6k, uncapped) holds +50% structure-held information
  at *half* the seed's bonds, i.e. fitness climbed by reducing bond
  mass. The screen's #1 detonated in confirmation for the second search
  running — `confirm_top ≥ 2` is empirically the floor. The anatomy
  question above was **answered 2026-07-15**
  (docs/research/sweeps/2026-07-15-headline-anatomy.md): it structurally
  favors condensation — a per-term decomposition of complexity on real
  structures (observer instrument extension) plus a maximum-entropy
  ceiling shows no structure respecting the condensation mark can beat
  actual's 3631 at any observed population (marked-sparse ceiling
  3598–3614; at observed degree entropy the requirement is a single
  ≥43k-particle structure, 4× anything ever simulated). The 20k
  leaderboard is a condensation ladder, and the champion itself crosses
  the mark between 3k and 20k. This is the evidence-backed
  missing-ingredient statement the exit criterion names as its second
  branch; whether the criterion is amended (bounded-degree headline),
  kept, or reinterpreted is owner-level and parked as Q-2026-07-15-A —
  the phase is not marked done on the strength of this session's own
  analysis. **Fourth run (search-04) 2026-07-17**
  (docs/research/sweeps/2026-07-16-search-04.md, spec committed
  2026-07-16): three search-03 seeds raced gate count (2 / 3 / 0
  env-gated culls) in one pool — the 2-gate line swept; the gates
  themselves stayed bit-frozen for eight generations while every
  late-generation fitness leap came from `RewireCondition` moving
  information bounds onto matter/energy (the same rewire found
  independently three times) — under an information-rewarding fitness,
  evolution *removes* information preconditions to widen
  participation. The screen's #1 **and** #2 both detonated in
  confirmation (confirm_top 3 is now the empirical floor, third
  screen-champion detonation running); the honest champion g008-i003
  confirms at 94.80 (+6.6 % over search-03) on an eighth of the bond
  mass. The env-gate mutation-operator case is weaker, not stronger:
  nothing pressed against the frozen climate window. The champion's
  20 k evaluation (affordable for this lean family: 212 s) scores
  headline 2 192.4 — below search-02's 2 421.6 and the condensing
  packs — while posting the highest fitness ever measured (106.97):
  fitness and the raw exit scalar now disagree on measured numbers,
  the concrete form of Q-2026-07-15-A. **Fifth run (search-05)
  2026-07-20** (docs/research/sweeps/2026-07-20-search-05.md): the
  screen moved to the detonation horizon — 6 k screens with the
  6 k-scale cap (600 k) as the screen's own breaker, 12 k confirms at
  1.2 M, search-04's honest champion racing both of its detonators
  in one pool. Both detonators capped mid-screen at g000 (fitness 0,
  culled at first selection); mutation re-discovered detonation six
  times across g001–g004 and the screen killed every one
  same-generation; the final two generations were cap-free. **All
  three confirmations held — the first search whose screen #1
  survived confirmation** (g005-i003: 970.22 at 12 k, fitness
  152.61, bounded == raw). The de-informationalizing rewire of
  search-04 reversed: it was a 3 k-horizon artifact, and 6 k-sampled
  fitness instead drove information to clamp scale (structures
  averaging ~0.76 × `information_max` per member) — recorded
  instrument limit: past the clamp, fitness v1's information term
  stops discriminating (a future fitness fork, parked in the
  findings doc, no change made).
- **Selection-pressure experiments**: **first pack shipped 2026-07-13**
  (`packs/sieve.ron` + `configs/sieve.ron`) — information gates survival
  (info-poor particles are absorbable), reproduction (emit requires an
  information floor), and structure itself (bond create/break are
  information-gated), against a config-set information decay that makes
  holding information cost energy forever. The deliverable's schema
  question is answered: the existing rule vocabulary expresses the
  coupling — no engine extension was needed. **Second pack shipped
  2026-07-15** (`packs/gradient-sieve.ron` + `configs/gradient-sieve.ron`
  + docs/research/sweeps/2026-07-15-gradient-sieve.md): the sieve plus
  one env-gated cull whose information floor is 2× higher where the
  field is high — selection *strength* varies across space with no rule
  mentioning position, answering the env-differential half of the
  deliverable. Causally pinned by a two-uniform-worlds test (an open
  gate costs population and thins the targeted information band); key
  finding: the sieve *recycles*, so harder local selection drives
  faster local turnover rather than emptier local statistics — a
  process difference snapshots hide. At the 3k gate: fitness 76.09
  (above sieve's 75.70) from 7% fewer bonds. Still open: whether
  searched variants of env-selection packs find qualitatively new
  dynamics — **answered 2026-07-15 by search-03**
  (docs/research/sweeps/2026-07-15-search-03.md): with sieve and
  gradient-sieve competing in one pool, the cline-world lineage swept
  ranks 1–26, generation 3 *duplicated* the env-gated cull under
  uniform DropRule pressure (every top-10 pack carries 2–3 gates where
  the seed had one), and a fully de-gated branch survived but never
  led. The deliverable's both halves are now answered; remaining
  follow-ups (gate divergence under longer searches, 20k confirmation
  of the regime family) are recorded in the findings doc.
- **Findings docs**: each sweep lands docs/research/sweeps/<date>-<topic>.md
  with the config corpus, scores, and what was learned — negative results
  count and prevent re-running dead regions.
- **Bounded-headline measurement 2026-07-17** (Q-2026-07-17-B,
  docs/research/sweeps/2026-07-17-bounded-headline.md): every RunScore
  now reports the headline restricted to non-condensed rows, and the
  affordable corpus subset + both discovered champions were re-scored
  at 20 k to populate it — the measured evidence Q-2026-07-15-A was
  waiting on. The exit criterion stays scored on the raw scalar until
  the owner decides.

Exit criteria: at least one discovered (not hand-authored) regime whose
Observer metrics beat every shipped pack on persistence × complexity, with
its ancestry reproducible end-to-end; or a documented, evidence-backed
statement of which ingredient is missing for that to be possible.

Non-goals: no engine/physics changes to chase scores (park proposals as
questions); no distributed infrastructure — one box, sequential runs.

## Phase 7 — AI Narration and Hardening

Goal: the storyteller layer, plus release-quality robustness.

Deliverables:

- AI narration consuming Observer output only: summaries, reports, Q&A about a run's history. Engine remains fully functional with AI disabled.
- Shareable replays (seed + config + rule-pack hash + player actions).
- Save-format migration between engine versions.
- Performance hardening against Phase 2 baselines; long-run soak tests.

Exit criteria: a complete run can be narrated as a history; disabling AI changes nothing but the narration.

## Open design questions

Decide when the phase that needs them starts, not before:

1. **Information semantics** (Phase 3): what does a particle's information quantity actually do — copyable at a cost? decays? gates interactions? This is the single most emergence-critical decision in the project.
2. **Float policy** (Phase 2): same-machine determinism allows plain f32/f64, but parallel reductions still need ordering discipline; decide summation strategy when conservation accounting is built.
3. **Rule format** (Phase 3): custom binary/RON schema vs. embedded scripting for conditions. Bias toward data, not scripts — scripts make replay identity and sandboxing harder.
4. **Snapshot mechanism** (Phase 5/6): how Observer and renderer read state without stalling the simulation (double-buffering vs. copy-on-write).
5. **Bond storage** (Phase 3): particle graphs are cache-hostile; decide layout (adjacency in components vs. separate bond store) when bonds land.
