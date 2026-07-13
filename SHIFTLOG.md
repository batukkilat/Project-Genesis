# SHIFTLOG — what each shift did, proven how, and one thing worth learning

Newest first. Format defined in GOAL.md (§ Shift log). Written for the
owner: full technical vocabulary, explained rather than simplified.

---

## 2026-07-13 — cloud night shift 1 (Fable)

Commits pushed: `fcc4032` run scoring (Q-2026-07-13-A), `4ff13d9` sweep
driver (Q-2026-07-13-B), `20a4efa` sieve selection-pressure pack,
`cc03d10` search-loop design doc, `15e968b` baseline sweep findings +
score corpus, `84b1679` search mutation operators + ancestry,
`828fa95` search fitness v1.

**What changed and why.** Phase 6.5 (the experiment loop) went from a
roadmap section to a working instrument chain. First, *run scoring*: a
run's observer timeline — the per-sample record of structures, metrics,
and hypotheses — now collapses into one flat RON `ScoreRecord`: an
identity stamp (seed, tick count, sample cadence, final state hash — the
replay fingerprint) plus final-and-peak aggregates and one headline
scalar, the maximum of persistence × complexity over every structure at
every sample. Peaks matter because a regime that flourishes and
collapses mid-run would be invisible to end-state-only scoring. Second,
the *sweep driver*: `genesis sweep` runs an explicit list of
(config, pack, script) triples through the exact same scoring code path
and writes per-run records plus a comparison table sorted by score —
batch order structurally cannot influence any output. Third, the
*baseline*: the full shipped-pack corpus was swept at 20k ticks; the
findings (below) reshaped the search design. Fourth, *selection
pressure*: the new `sieve` pack makes information — until now a passive
quantity — determine survival (info-poor particles are absorbable),
reproduction (only info-rich split), and structure membership
(bonds form between info-rich, break toward info-poor), while the paired
config's `information_decay` makes information leak, so keeping it costs
energy forever. Notably this needed *zero* engine changes — the existing
rule vocabulary already expresses information-gated survival, answering
the deliverable's standing schema question. Fifth, the *search
foundation*: schema-bounded mutation operators (multiplicative jitter,
rule drop, duplicate-and-jitter — the gene-duplication analog — and
condition rewire) that repair-clamp into validation bounds; RON ancestry
sidecars recording parent, exact operator, and RNG derivation
coordinates, so every mutant is reproducible; and fitness v1, a product
of saturating terms designed directly against the baseline's main
finding.

**How it was proven.** Determinism at every layer: the score
integration test runs the full pipeline twice from scratch and requires
bit-identical scores; `genesis score` run twice on chains produced
byte-identical records; mutants are pure functions of
(seed, generation, individual) — re-running produces diff-identical
files; 1000 five-step mutation chains across a five-pack corpus stay
schema-valid and assembly-safe. The sieve pairing passed the standard
four-way verify (two fresh runs, save/resume, single-thread → one hash).
The fitness function is tested *against the committed baseline records*:
the raw scalar ranks condensed worlds on top, fitness provably inverts
that ordering — the test is the design rationale, executable. What these
cannot catch: whether the scored aggregates actually track "interesting"
emergence (that is a judgment the sweep findings inform but cannot
settle), and any cross-machine variation (scores inherit same-build
determinism only — a different box may hash differently; records stamp
that).

**What to watch.** (1) The baseline's central finding: the headline
persistence × complexity scalar is currently a *condensation contest* —
`actual` and `bands` top the leaderboard by welding half the world into
one immortal mega-blob with 1–2M accumulated bonds. The phase exit
criterion is defined on that scalar, so "beating every shipped pack"
must not be read as "out-condense them"; fitness v1 exists precisely to
steer search elsewhere, and its exact form is still unratified — the
first real search run should stress it. (2) Four shipped regimes
(diffusion, hoarders, churn, echoes) score zero because all v1 observer
metrics are bond-graph facts; order living in quantity distributions is
invisible to the instrument. Recorded as a boundary, may eventually
justify a new metric family (QUESTIONS.md candidate, not a quiet
extension). (3) The original echoes sweep pairing was a *null run* — the
default config spawns zero information, so its imprint rule never fired;
fixed in the spec. Any future zero should be checked for "did the rules
ever fire". (4) The sieve 20k-tick score run was still executing at
shift end (bond-dense regimes take tens of minutes to hours — wall time
tracks bond count, not particle count); its row lands in a follow-up.
Re-run if lost: `genesis score --config configs/sieve.ron --rules
packs/sieve.ron --ticks 20000 --every 100 --out sieve.score.ron`.
(5) `sweeps/shipped-packs.ron` reruns bands at 108 minutes — do not
casually re-sweep the corpus; the committed records are the baseline.

**Concept of the shift: why a fitness function is not a score.** The
scorer reports what happened — neutral aggregates, no preferences. The
search loop needs something different: a single number whose *gradient*
points where we want exploration to go. Using the report directly as
fitness looks natural and fails subtly: our headline scalar rewards
persistence × complexity, and the cheapest way a mutating rule pack can
maximize it is to bond everything into one eternal lump — technically
persistent, technically complex, scientifically dead. This is Goodhart's
law in miniature: when a measure becomes a target, it stops measuring.
The defense is structural, not moral: build the target out of *saturating*
terms (logarithms that flatten as any one axis grows), so the only way to
climb is to be good at several things at once — many structures AND long
lifetimes AND retained information — while the neutral report is still
recorded beside it, unbent, for the exit criterion to judge. One number
to climb, another to trust; keeping them separate is the whole trick.

Commits pushed: `06417bb` rotation as frame spin (save v15), `51f1ba9`
windowed app shell, `c312be6` fps cap, `5cd4ec1` Windows launcher,
`5874a6e` system requirements, `6b33483` PLAYBOOK, guide plan, Phase 6.5.

**What changed and why.** Two big things. First, planet rotation entered
the physics as *frame spin*: instead of moving the world, we work in a
rotating reference frame, where every particle feels a Coriolis
deflection — a force perpendicular to its velocity, proportional to speed
and to the spin rate Ω. This is the "f-plane" approximation from
geophysical fluid dynamics: the standard way rotation enters a flat 2D
domain. It was implemented not as a force added to the integrator but as
an *exact rotation of each velocity vector* by angle −2Ω·dt each tick.
Second, the windowed app (`genesis-app`) landed: a Bevy shell that owns
the simulation, runs whole ticks per frame via the warp pacer, and draws
the extraction output (sprites+bonds zoomed in, heatmaps zoomed out).

**How it was proven.** Energy conservation test (Coriolis force is always
perpendicular to motion, so it can do no work — kinetic energy must come
out bit-identical, and does); momentum-magnitude test (total momentum
rotates under spin but |P| stays constant — the invariant moved, it didn't
vanish); replay-identity tests (zero spin hashes exactly like the
pre-feature build; nonzero spin is a different universe from tick 0);
save/resume mid-spin; thread-count invariance; `genesis verify` four-way
on scripts/spin-up.ron. What these cannot catch: whether the *emergent
consequences* of spin (chiral drift, band formation) are physically
sensible — that needs long runs and eyes, not unit tests.

**What to watch.** The app was only verified under software rendering
(WSLg/llvmpipe); real-GPU behavior — tier thresholds, texture upload
performance — is unconfirmed until the Windows test. The wgpu "gles"
feature shim in genesis-render's Cargo.toml exists only to force a
backend into the shared wgpu build; if bevy ever forwards that feature
itself, the shim becomes redundant and should be removed.

**Concept of the shift: why an exact rotation instead of a force.** The
naive way to apply Coriolis is explicit Euler: `v += a·dt` each tick. But
that formula moves the velocity along a *tangent line* to the circle it
should be tracing, so every tick the speed grows slightly — energy is
injected from nothing, forever, and the error compounds. The alternative:
since Coriolis only ever *turns* velocity without changing its length,
apply the turn exactly — multiply the velocity by a rotation matrix for
the precise angle covered in one tick. Speed is preserved to the last
bit, and the method is stable at any dt. Plasma physicists call this the
Boris pusher. The general lesson (PLAYBOOK §3): when a continuous law
conserves a quantity, choose the discrete formula that conserves it
*exactly*, then test the invariant — never settle for "small error per
tick," because ticks are the one thing this project has millions of.
