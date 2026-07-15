# Gradient sieve — the environment differentially selects

Second selection-pressure pack (Phase 6.5 deliverable), closing the
"regimes where the *environment* differentially selects" gap the sieve
findings left open. `packs/gradient-sieve.ron` is `packs/sieve.ron` plus
exactly one rule: a second CULL whose information floor is 0.45 instead
of 0.2, gated to fire only where env field 0 ≥ 0.6. Paired with
`configs/gradient-sieve.ron` (the sieve world plus one west→east
`GradientX` field), the world becomes a cline of selection strength; no
rule anywhere mentions position. As with the sieve, the existing rule
vocabulary expressed the coupling — no engine extension was needed
(env-gating absorb works exactly like env-gating bond_create in bands).

Machine: 4-core Intel Xeon @ 2.80 GHz (cloud box), release build,
rustc 1.96.1. Score record: `2026-07-15-gradient-sieve/`.

## Finding 1 — the field value causally sets selection strength

Pinned by `gradient_sieve_field_value_sets_selection_strength`
(crates/genesis-sim/tests/packs.rs): the same pack on two worlds
differing *only* in the field's uniform value — gate closed (0.0) vs
open (1.0); same rule list, same RNG stream layout, same initial state.
At tick 600 (3 000 particles, 548², decay 0.02): closed
n = 3 854 / band(0.2–0.45) = 136; open n = 3 730 / band = 105. The open
gate costs ~120 particles (the extra absorptions) and thins the targeted
information band by a third. The field value is the entire difference
between the two universes, so the differential-selection claim is causal,
not correlational.

## Finding 2 — local selection strength does not print onto local statistics

The first version of that test asserted the *within-one-world* regional
signature instead — sub-floor particles rarer in the high-field east
than the low-field west — and the equilibrium refused to cooperate: at
600 ticks (5 000 particles, shipped-like density), the east's sub-floor
fraction under the gradient pack was statistically indistinguishable
from the plain-sieve control's east (0.062 vs 0.061), while population
counts even ran *higher* in the east than the west. The mechanism, once
seen, is obvious and is the actual lesson of this pack: **the sieve
recycles**. An absorption hands the victim's matter and energy to an
info-rich survivor, which crosses SPLIT's fuel threshold sooner, and its
child — carrying half of an ≥ 0.6 parent's information — lands at
~0.3, *inside* the very band the cull empties. Harder local selection
therefore drives faster local turnover, not a proportionally emptier
band: removal and refill rise together. Spatial standing statistics are
an equilibrium of the whole loop, and the environment's knob turns the
loop's *rate* more than its fixed point. (This echoes the search-02
observer lesson — the metric family reads states, and states can hide
process differences; turnover is invisible to a snapshot.)

## Finding 3 — the 3 k gate row: slightly fitter than sieve, at lower bond cost

`genesis score --config configs/gradient-sieve.ron --rules
packs/gradient-sieve.ron --ticks 3000 --every 100`:

| | gradient-sieve | sieve (2026-07-14 gate row) |
|---|---:|---:|
| headline (persistence × complexity) | 294.50 | 318.55 |
| search fitness v1 | **76.09** | 75.70 |
| bonds @3 k | 154 344 | 165 714 |
| particles @3 k | 13 707 | 13 866 |
| structure-held information @3 k | 7 287 | 7 326 |
| structures final / peak | 837 / 1 781 | — / — |

The added pressure trims the headline (fewer mid-band particles means
slightly smaller/looser webs — headline structure 95 members vs sieve's
top complexity rows) but *raises* fitness v1: essentially the same
retained information and structure count from 7 % fewer bonds. A
harsher environment producing a leaner-but-equally-informed world is
exactly the kind of tradeoff the selection-pressure experiments exist
to surface. Both specs now carry a `gradient-sieve` row
(3 k-capped in the 20 k spec — same 150 k-bond class as sieve, same
affordability reason).

## Open

- **Searched variants** (the deliverable's remaining half): does
  `genesis search` seeded from gradient-sieve exploit the gradient —
  e.g. by tuning thresholds so the mild west becomes a nursery whose
  emigrants survive the east — or does it just strip the env rule?
  The mutation operators keep `env_cond` intact (they never rewire
  env gates), so the gradient stays a fixed feature of the child's
  world; a run of the search-02 shape costs ~70 min on this box.
- **Seeing the cline**: turnover-vs-standing-stats (finding 2) is
  invisible headless but should be directly visible as regional churn
  in the Phase 6 app's T2/T3 heatmaps — worth a look in the next
  desktop session (renderer already ships; this is a play-session item,
  not a work item).

## Reproduce

```sh
cargo test -p genesis-sim --test packs gradient_sieve
genesis score --config configs/gradient-sieve.ron --rules packs/gradient-sieve.ron \
              --ticks 3000 --every 100 --out gradient-sieve-3k.score.ron
genesis verify --config configs/gradient-sieve.ron --rules packs/gradient-sieve.ron --ticks 500
```
