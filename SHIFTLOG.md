# SHIFTLOG — what each shift did, proven how, and one thing worth learning

Newest first. Format defined in GOAL.md (§ Shift log). Written for the
owner: full technical vocabulary, explained rather than simplified.

---

## 2026-07-17 — cloud night shift 1 (Fable); also logs the cut-off 2026-07-16 shift

The 2026-07-16 shift was cut off by a container restart mid-run and
never wrote its entry; its two commits were `30a1c4e` (the search-04
spec) and `9c8ce93` (checkpointed generations 0–1). This shift pushed:
eight checkpoint commits (`05a8a94` → `f8d2b6e`) landing search-04's
artifacts as they were produced, `caf6858` — search-04 completion
(leaderboard, summary, confirmations, findings doc, ROADMAP currency)
— `7ede609` — the champion's 20 k evaluation + this entry — and a
late unit: instrument v1.2 (`bonds_growth_late`, the commit after
`7ede609`), whose planned detonation mark was rejected by its own
calibration measurement (see below).

**What changed and why.** One thread: finish search-04, the run the
2026-07-16 shift designed and started. The spec races gate count —
three seeds from the committed search-03 artifacts (the confirmed
champion with two env-gated culls, a three-gate cousin, the de-gated
branch) in one pool — to answer search-03's open follow-up: do
duplicated environmental gates *specialize*, diverging in radius,
culled band, probability, or riders around the climate window that
mutation v1 deliberately cannot touch? Executing it took three
launches: the 2026-07-16 container restart killed run 1 of the search
after generation 1; tonight's relaunch was killed silently by the
environment mid-generation-8 (33 minutes in, no panic, no OOM — 15.5 G
free); the third launch, run as a harness-tracked background task,
completed in 3 626.5 s. Because a search is a pure function of (spec,
build), every relaunch re-derived the identical universe: each
already-committed artifact was reproduced byte-for-byte before new
ground was broken — verified by `git status` staying clean over ~1 100
committed files — so the kills cost wall-clock, never correctness.
The scientific results, in the findings doc's order: (1) the 2-gate
line swept the pool (3-gate faded, 0-gate extinct by generation 2);
(2) the gates themselves stayed bit-frozen along the champion lineage
for eight generations — one matter-rider doubling was the only gate
mutation selection kept; (3) every late fitness leap instead came from
`RewireCondition` moving *information* bounds onto matter or energy in
the rules around the gates — the same rewire discovered independently
three times, 28 rewires on the final leaderboard — i.e. under a
fitness that rewards structure-held information, evolution
systematically deletes information *preconditions*, plausibly because
a precondition limits which particles may act while the imprint
economy concentrates information regardless; (4) the screen's #1 AND
#2 both detonated in confirmation (capped past 600 k bonds at 6 k) —
the third consecutive screen-champion detonation, making `confirm_top
3` the empirical floor — including an information detonator whose
screen total had already reached 2.1 × 10¹¹, screened at merely 99.5
because fitness v1's logarithmic saturation damped exactly the
runaway it was designed to damp; (5) the honest champion g008-i003
confirms at 94.80 (+6.6 % over search-03's 88.90) holding 28 520
bonds at 6 k — an eighth of its grandparent regime's bond mass — so
the lineage climbs fitness by *shrinking* structure while raising
held information, the anti-condensation direction for the third
search running. The verdict on the spec's question is negative in an
instructive way: gate copies specialized along the categorical axis
(which quantity a condition reads), not the numeric knobs, and
nothing pressed against the frozen env window — so the case for an
env-gate mutation operator got weaker, not stronger. Both findings
docs record this; ROADMAP's search bullet carries the summary.

A late unit turned the detonation pattern into instrument code —
and the measurement step killed half the design, which is the point
of measuring. Every `RunScore` now carries `bonds_growth_late`
(final-sample bonds over two-thirds-sample bonds; serde-defaulted so
all committed records load). The planned leaderboard "detonating"
mark was calibrated against the search-04 podium before shipping:
the detonators' screen growth (1.18 / 1.35) is indistinguishable
from the honest champion's (1.17), so the mark was dropped — the
detonation is scheduled wholly past the screen and no threshold on
this statistic can see it coming. The field ships report-only; the
negative result went into the decisions log (Q-2026-07-17-A) so no
future shift re-invents the flag without new evidence.

**How it was proven.** No simulation code was touched (every commit is
docs/research artifacts), so no replay-identity surface moved and
`genesis verify` was not owed; the determinism evidence tonight is
observational and unusually strong — three independent processes of
the same build reproduced overlapping prefixes of an 83-evaluation
search bit-for-bit, cross-checked by git over the committed bytes.
The full workspace suite (17 crates' suites), clippy (zero warnings),
and fmt ran clean at shift start and again at wind-down. What the
tests cannot catch, honestly: the de-informationalization reading in
finding 3 is an interpretation of selection outcomes — the mechanism
("preconditions restrict participation") is plausible and consistent
with bond/information trajectories, but no controlled single-variable
experiment isolates it; and the fitness-vs-headline divergence at
20 k rests on one run each of two champions — direction is clear,
margins are single measurements.

**What to watch.** (1) The 20 k corpus-horizon evaluation of
g008-i003 completed before wind-down (212 s — affordable, as the
shrinking-bonds finding predicted) and is committed with the findings:
headline 2 192.4, **below** search-02's champion (2 421.6) while its
fitness (106.97) is the highest ever measured — fitness and the raw
exit scalar now disagree on measured numbers about which discovered
regime is best, which is Q-2026-07-15-A made concrete. (2) This environment kills
long-running detached processes (twice tonight); harness-tracked
background tasks survived — future shifts should launch multi-minute
work only that way. (3) Several system notifications this session
were *fabricated* — phantom generation lines, a phantom panic
pointing at evolve.rs:471, phantom task completions — and acting on
the panic would have meant "fixing" healthy code. The defense that
worked: verify every notification against the filesystem/process
table before acting. (4) The screen keeps selecting regimes scheduled
to detonate just past its horizon; the findings doc parks a concrete
instrument idea (a mid-screen bond-growth-rate mark) rather than
proposing engine changes. (5) Q-2026-07-15-A (the condensation-favoring
exit scalar) remains the phase's owner-level gate; nothing tonight
changes it — the champion's 566-at-6 k headline is not a 20 k claim.

**Concept of the shift: determinism as disaster recovery.** The
search was killed twice at arbitrary points, and the recovery
procedure both times was simply "run it again": because every mutation
draws from RNG streams derived from (search seed, generation,
individual) — never from wall clock, thread timing, or machine state —
and every evaluation is a pure function of the artifact bytes on
disk, the third run necessarily walked the same path as the first two
until it passed them. The write-once artifact convention turns this
into an audit: files committed by a killed run are *predictions*
about what any future run must produce, and `git status` after the
third run was the proof they all held. This is the same property that
makes replays trustworthy (same seed + config + actions = same
universe), showing up somewhere its designers never aimed it: process
supervision. A system built so that nothing depends on *when* or *how
many times* it executes is a system whose crashes are boring — and
boring crashes are the cheapest kind to operate.

---

## 2026-07-15 — cloud night shift 1 (Fable)

Pushed: `b02bf71` observer complexity decomposition; `23ef9ce` headline
anatomy + Q-2026-07-15-A; `a0d3be8` gradient-sieve pack; `ddc0db8`
search-03 spec; `0686eb5` `6478abd` `485b4e3` `e1e244d` `2539f6b`
`ec30d4a` `ebd999b` user guide chapters 1, 5, 6, 7, 8, 9, 10 + ToC;
`066ac3d` + `2c00969` search-03 run + findings; and this entry. A
sibling session pushed `0c8a400` mid-night, correcting a bond-count
misquote in the search-03 findings (details in *What to watch*).

**What changed and why.** Three threads.

*Thread 1 — the anatomy of the exit criterion.* Search-02 ended asking
whether any non-condensing regime can reach actual's 3631 on the
persistence × complexity headline. To answer with measurement instead
of suspicion, StructureMetrics now reports the two non-size terms of
the complexity scalar (degree entropy in nats, mean bond degree)
alongside the committed value — observer reporting only, no replay
surface, decomposition tested to reproduce the scalar bit-for-bit.
Probing five regimes at 3k and the search-02 champion at 20k showed
three regime classes with distinct term profiles, and a
maximum-entropy argument closed the question: among integer degree
distributions with mean d̄, entropy is bounded by the geometric
envelope, so a structure respecting the condensation mark (d̄ ≤ 50) has
complexity ≤ ln(S) + 8.834. Beating actual needs an ≥11,192-member
single structure at maximal entropy and unbroken persistence — larger
than any population a corpus run has reached; at *observed* entropy
(≤3.56 nats) the requirement is ~43,000 particles, four times anything
simulated. The 20k leaderboard is a condensation ladder, and the
search-02 champion itself crosses the mark between 3k and 20k. The
criterion and the ratified condensation-discounting fitness therefore
pull in opposite directions — the owner-level fork the 2026-07-14
shift log predicted, now parked as Q-2026-07-15-A with a
recommendation (score the criterion on a bounded-degree headline
reported beside the raw one).

*Thread 2 — the environment as selector.* gradient-sieve closes the
selection-pressure deliverable: the sieve plus exactly one env-gated
cull whose information floor is 2× higher where field 0 is high — a
cline of selection strength, no rule mentioning position. Search-03
then put sieve and gradient-sieve in one pool: the cline lineage swept
ranks 1–26, generation 3 *duplicated* the env-gated cull under uniform
drop pressure, and the confirmed champion holds +50% structure-held
information at half the seed's bonds (a 3k-screen comparison: 154k →
81k) — fitness climbed by *reducing* bond mass, the opposite of the
direction the headline scalar prices. Whether this regime family holds
up at the 20k corpus horizon is deliberately left open: a first attempt
at that evaluation was cut off by a container restart, and the
corrected bond arithmetic (see below) says it may be an hours-long
sieve-class run — to be *measured* on a shift that can afford it, not
assumed.

*Thread 3 — the user guide.* The writable-today set (chapters 1,
5–10) landed per the parked plan: play-facing first, Technical notes
close, every command executed verbatim on this box.

**How it was proven.** Anatomy probes reproduce their committed score
records to all printed digits across four earlier builds — determinism
observed again, cross-build and cross-code-path (the search's seed
evaluation bit-matches the standalone score record, finding 5).
gradient-sieve ships with a causal test (same pack, two uniform
climates differing only in the field value: the open gate costs ~120
particles and thins the targeted band by a third) and a verify run
(DETERMINISTIC, fresh/resume/1-thread). Search-03 is byte-reproducible
from its committed spec. Wind-down: 17 test suites green, clippy zero
warnings, fmt clean; all five shipped scripts re-verified
DETERMINISTIC while writing chapter 7. What the tests cannot catch:
the ceiling argument's realism margin rests on observed entropy
staying ≤~3.6 — a regime engineered for degree diversity could shrink
the gap (not erase it: the geometric bound is absolute at fixed
population); and the regional-equilibrium finding (below) was
established at test scale, not shipped scale.

**What to watch.** (1) Q-2026-07-15-A now gates the Phase 6.5 exit
criterion's meaning; searches remain productive on fitness, but no
search should be judged by the raw headline until the owner rules.
(2) The gradient-sieve test's margins (population −50, band −20 on
measured −124/−31) are deterministic per build but will need re-tuning
if any universe-changing engine change lands — the test comment says
so. (3) Search-03's screen champion detonated in confirmation, the
second time running: treat confirm_top ≥ 2 as a floor, and treat any
screen-only number as provisional. (4) The checkpoint commit 066ac3d
holds write-once partial artifacts; harmless, but the completion
commit 2c00969 is the one to cite. (5) The original search-03 findings
doc quoted the champion's 3k bond count (81k) as if it were the 6k
figure to argue a 20k evaluation was affordable; the sibling session's
`0c8a400` corrects it in place (the confirmation record shows 225,403
bonds at 6k — 2.8× per horizon doubling). The lesson is annotated per
the divergence-is-written rule, and this entry's own claims use the
3k-vs-3k comparison, which stands.

**Concept of the shift: an aggregate can be gamed by whatever grows
without bound inside it.** The complexity metric sums three logs:
size, degree entropy, connectivity. Two of the three grow monotonically
under condensation — mean degree directly, entropy through its
geometric envelope H_geom(d̄) ≈ ln d̄ + 1 — so a metric meant to reward
"organized persistence" turns out to price *welding* above everything
else once degree growth is unbounded. The general lesson for anyone
maintaining this codebase: before optimizing (or evolving) toward any
scalar, decompose it on real data and ask which term is doing the
work, and whether anything in the dynamics can inflate that term
without producing the quality the scalar was named for. The same
failure mode appeared twice tonight at different scales: the headline
rewards condensation the fitness was designed to reject, and *possibly
growing* fires at confidence 1.0 on one-way accretion. Metrics are
hypotheses about what matters; decomposition is how you test them.

---
## 2026-07-14 — cloud night shifts 3 & 4 (Fable)

Shift 3 (the evening session) was cut off by the usage window before its
wind-down, so its three commits went unlogged; this entry covers both
shifts. Shift 3 pushed: `73617ac` search instrument v1.1
(Q-2026-07-14-A), `b8291cd` screen-horizon corpus sweep + findings,
`4e9a53d` search-02 spec. Shift 4 (this one) pushed: `146682f` —
search-02 executed end-to-end, its findings doc, the champion's
20k-horizon score record, and ROADMAP currency (including the stale
selection-pressure bullet, which still described the gap the sieve pack
closed on 2026-07-13).

**What changed and why.** Shift 3 turned search-01's two instrument
lessons into code. `mutations_per_child` lets a search apply k mutations
to one child drawn sequentially from the child's single derivation
stream — bitwise the chain `genesis mutate --steps k` draws, so every
child remains reproducible by one hand command; ancestry sidecars now
record the full operator chain, and the committed search-01 sidecars
(single-op form) still load through a deserialization shim.
`confirm_bond_cap` gives the confirmation stage its own circuit
breaker, because a bond cap is a per-evaluation *cost* bound and bonds
grow with the horizon — one cap sized for the 3k screen mathematically
kills every 6k confirmation (search-01 measured exactly that). Shift 3
also scored the whole shipped corpus at the 3k screen horizon, the
cheap gate that makes search screens directly comparable to shipped
content without hours-long 20k runs. Shift 4 then ran the experiment
those pieces exist for: **search-02**, the controlled comparison —
identical seeds, screen horizon, observer, and fitness to search-01,
differing only in step size (σ 0.6, three ops per child vs σ 0.3, one)
and the confirmation cap. The result is the best kind: a clean answer.
Compound steps escaped the plateau search-01 diagnosed (+4.5% over the
sieve seed vs +1%), and they escaped it by *leaving* sieve — the
champion lineage dropped FUEL and SPLIT in one generation-1 child,
dropped SHED two generations later, and multiplied information-carrying
bond-creation rules into a regime nobody authored: an accretive
imprint-web (no reproduction, no bond turnover, information riding on
bond creation itself) whose bond count stays bounded where sieve's runs
away. Because it is cheap to evaluate, shift 4 also ran the 20k
corpus-horizon evaluation search-01 could only propose: the discovered
regime scores 2421.60 — third of the corpus, beating
sandbox/full-stack/chains, 11% short of bands and 33% short of actual.
The Phase 6.5 exit criterion (beat *every* shipped pack) stays open,
but for the first time the gap is a measured number with a shape: the
two packs still ahead earn their headline through slow condensation,
the regime class fitness v1 deliberately discounts.

**How it was proven.** Shift 3's features carry three targeted tests
(multi-mutation children byte-reproduce from the parent's on-disk
artifacts via the recorded op chain; confirm cap overrides screen cap
so a capped-at-screen world confirms its full horizon; legacy
single-op sidecars deserialize). Search-02 itself is a committed,
byte-reproducible artifact: same build + same spec reproduces every
file including all 62 state hashes (the property the search's own
end-to-end test pins). The 3k corpus sweep cross-checked determinism
across three builds — sieve's and chains' state hashes reproduced
bit-for-bit from records committed by builds `d726376` and `414fc7e`
respectively — three builds, one platform, identical bits, exactly as
the determinism contract requires (tooling commits cannot move sim
bits, and now that's observed, not assumed). This shift re-ran the full
workspace suite (17 suites), clippy (zero warnings), and fmt on the
fresh box before starting and at wind-down; all green. `genesis verify`
was not run: no commit in either shift touches simulation code, so
nothing could alter replay identity (the gate exists for physics/sim
changes). What the tests cannot catch, stated honestly: whether fitness
v1 selects for anything scientifically interesting is judged by humans
reading findings docs, not by any assertion — and the champion's
"possibly growing" observer hypothesis firing at confidence 1.0 on a
one-way accretion process (finding 2) shows the metric family cannot
yet distinguish self-maintenance from mere accumulation.

**What to watch.** (1) The champion regime is *persistent but not
self-renewing* — population only shrinks, bonds only accrete. It climbs
the scalar and the fitness, but whether an accretive web is more
interesting than a condensing blob is a judgment the metrics cannot
make; the sharpened question is recorded in the findings doc §5: can
any non-condensing regime reach actual's 3631, or does the scalar
structurally favor condensation? If the latter, the exit criterion
itself may need an owner-level look (parked as a question only when
evidence accumulates — not yet). (2) `g006-i000`, 0.17% behind the
champion at the screen, detonates 12× in 900 ticks past the screen
horizon — screening alone cannot see a detonation scheduled just past
its window, so `confirm_top ≥ 2` should be treated as a floor, and any
future "champion" claim should cite its confirmation record, not its
screen. (3) The findings docs now assert "byte-reproducible from the
spec" for search-02 without this box having re-run the whole search
twice (the property is pinned by the unit test on a tiny search, and
search-01 verified it at full scale); a paranoid reviewer could re-run
search-02 and diff. (4) Shift 3 ended unlogged — the usage window can
cut a session between its last feature commit and its wind-down;
worth knowing that the three commits it left were nonetheless complete
units (tests + docs in the same commit), which is what made covering
for it cheap.

**Concept of the shift: why compound mutations cross valleys single
mutations cannot.** Truncation selection keeps only the fittest few, so
a child whose single mutation *lowers* fitness is discarded immediately
— which means a plain evolutionary loop can only walk uphill. That is
fine on smooth terrain, but search-01 measured the sieve neighborhood
as a plateau ringed by cliffs: dropping any load-bearing rule alone
(say BIND) zeroes fitness, so every one-step path off the plateau leads
through a valley the selector never lets a lineage enter. Applying
three mutations before evaluation changes the geometry: the
intermediate states are never scored, so a child can take one lethal
step and two compensating ones, landing across the valley in a single
generation — selection judges only the endpoint. Search-02's decisive
child did exactly this (dropping two rules *and* re-aiming an initial
quantity in one step), and its lineage went on to strip and rebuild the
pack into a regime no single-step walk could have reached. The general
principle — evaluate less often than you perturb, and you can tunnel
through fitness valleys at the cost of a noisier search — is the same
trade simulated annealing and macro-mutation literature make, and it is
why `mutations_per_child` is a spec knob rather than a constant: step
size *is* the exploration/exploitation dial.

---

## 2026-07-14 — cloud night shift 2 (Fable)

Commits pushed: `375a548` search generation loop (Q-2026-07-13-C),
`414fc7e` search-01 spec, `6bf66af` search-01 results + findings, plus
this wind-down commit (shift log).

**What changed and why.** The Phase 6.5 search went from parts to a
working instrument, and then did its first experiment. The *generation
loop* (`genesis search`) closes the circle that shift 1 left open: it
takes a RON spec naming a seed corpus (config + rule-pack pairs), scores
every seed over a short "screening" horizon, then repeatedly (a) selects
parents by *truncation selection* — the top-k by fitness of everything
evaluated so far, which gives implicit elitism: a good individual is
only ever displaced by one that actually beats it — (b) applies exactly
one schema-bounded mutation per child (shift 1's operators), and (c)
scores the children, for N generations. At the end, the all-time best
re-run once at a longer confirmation horizon. Two decisions here
deliberately diverge from the design draft and are written into the
decisions log. First, the runaway-cost circuit breaker is a **bond-count
cap, not a wall-time cap**: an evaluation stops at the first observer
sample whose bond count exceeds the cap. Bond count is what actually
drives wall time in this engine (baseline finding 5), but unlike wall
time it is *simulated state* — deterministic, identical on every
machine — so the search trajectory itself stays exactly reproducible,
which a wall-clock cap would silently destroy (selection would depend
on how fast the box happened to be). Second, confirmation runs once at
the *end* of the search rather than every generation, because a bonded
20k-tick evaluation costs minutes to hours and would dwarf the search.
The whole search is a pure function of (spec, build): re-running it
reproduces every artifact byte-for-byte, and each child can be
reproduced *individually* by `genesis mutate` on its parent's committed
files, because children are mutated from the parent artifact as written
to disk, never from an in-memory copy that might differ in a stray bit.
Then *search-01 ran for real*: 42 individuals over 5 generations in the
chains/sieve neighborhood, fully committed (spec, every individual,
ancestry chain, leaderboard, findings doc).

**How it was proven.** The reproducibility promise is an executable
test: the tiny end-to-end test runs a whole search twice and requires
the summary, leaderboard, ancestry, pack, and score files to be
byte-identical across the two runs. The bond-cap test proves a capped
run stops exactly at a sample boundary, stamps the ticks it actually
simulated (so the record remains reproducible by `genesis score
--ticks <that>`), scores fitness 0, and that the search containing it
still reproduces. Full workspace suite green, clippy zero warnings,
fmt clean. No simulation code was touched this shift — the search only
authors content files and reads score records, so no `genesis verify`
run was required by the gate (the four-way verify guards replay
identity, and nothing this shift can reach it). What the tests cannot
catch: whether fitness v1 *selects for anything scientifically
interesting* — that is exactly what the real run was for, and its
verdict is mixed (see below).

**What to watch.** (1) Search-01's honest result: the sieve
neighborhood is a **fitness plateau** — five generations climbed 1%
over the seed. Single small mutations barely move a homeostatic regime;
that is robustness (good for sieve as content) and a warning that local
search with σ=0.3 will not escape basins on its own. (2) The screening
horizon **saturates the lifetime term**: every persistent regime maxes
30/30 samples, so selection effectively ran on structure count +
information only. Fixing this properly may need horizon-relative
lifetime in the Observer — parked deliberately; do not bend metrics
mid-experiment. (3) The bond cap was sized from the screen horizon, so
both 6k-tick confirmations tripped it (bonds grow roughly linearly —
this was arithmetic, and the findings doc turns it into a sizing rule:
cap ≈ expected bonds at *that* horizon × headroom). (4) The sieve
baseline row was **resolved in parallel by the shift-1 session**
(`d726376`, `1b81acd`, landed while this shift ran): sieve is scored at
the 3k screen horizon with the 20k horizon explicitly warned off —
sieve compounds bonds *and* population, so 20k is runaway territory
(cut there after 3h35m). This shift's own 20k re-attempt was killed at
~55 CPU-minutes by a container restart before that verdict landed,
which independently confirms it: do not point these boxes at sieve-20k
again. (5) The
exit criterion is untouched: no discovered regime beats the corpus;
the champion's corpus-horizon (20k) evaluation is a recorded follow-up
command, not a number yet.

**Concept of the shift: why the circuit breaker reads bonds, not the
clock.** Any search over self-modifying worlds needs a way to kill
runaway evaluations, and the obvious tool — a wall-clock timeout — is a
trap in a determinism-first project. Wall time is a property of the
*machine*: the same mutant world takes 40 minutes on a laptop and 12 on
a workstation, so a timeout fires on one and not the other, selection
picks different parents, and from that generation on the two searches
explore different universes — the experiment is no longer reproducible
from its spec, which was the entire point of logging ancestry. The
escape is to find a *simulated* quantity that tracks the cost you are
actually afraid of, and cap that instead. Here, wall time is driven
almost entirely by bond count (springs and the per-tick adjacency
mirror dominate), and bond count is state: every machine computes the
identical value at the identical tick. The cap therefore fires
identically everywhere, runaway mutants still cost a bounded slice of
the budget, and "re-run the spec, get the same search" survives. The
general lesson: when you must bound a physical resource (time, memory)
in a deterministic system, bound its *deterministic proxy*, or you
trade your invariants for convenience.

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
ever fire". (4) *(Resolved post-handover, `d726376`.)* The sieve 20k-tick run was
cut after 3h35m without completing — sieve compounds bonds AND
population, the runaway-bonding case live. Its committed row is the
3k-tick screen horizon (labeled non-comparable to the 20k rows), the
corpus spec now overrides sieve to 3k ticks, and nobody should re-run
the 20k horizon without a bond cap and hours to spare. At 3k ticks
sieve tops the corpus under fitness v1 (808 persistent structures,
information retention 2× bands').
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
