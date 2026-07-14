# Sweep 2026-07-14 — the shipped corpus at the screen horizon (3k)

The 20k baseline (docs/research/sweeps/2026-07-13-shipped-packs.md) and a
search's 3k screens are not comparable: different horizons aggregate
different timelines, so search-01 could only relate its champion to the
corpus by proposing hours-long 20k evaluations (its finding 5). This
sweep scores the identical corpus at the search screen horizon — 3 000
ticks, observer sampled every 100 (30 samples), default `ObserverConfig`,
the exact settings of sweeps/search-01.ron and search-02.ron — so screen
results compare against shipped content directly, and a champion that
cannot beat the corpus *here* never needs a 20k evaluation at all. The
20k baseline remains the exit-criterion authority; this is the cheap
first gate in front of it.

Reproduce with:

```sh
genesis sweep --spec sweeps/shipped-packs-3k.ron --out <dir>
```

Machine: 4-core Intel Xeon @ 2.80GHz (cloud box), build `73617ac`
(rustc 1.96.1, release profile), all cores. Raw records live next to
this doc in `2026-07-14-shipped-packs-3k/`.

## Results

Sorted by the headline score (`max persistence × complexity`); fitness is
search fitness v1. Zero rows (churn, diffusion, echoes, hoarders,
physics-only) omitted — same set as at 20k, same reason (all v1 observer
metrics are bond-graph facts; baseline finding 2).

| run | score | fitness | structures fin/peak | largest fin/peak | info final | wall |
|---|---:|---:|---:|---:|---:|---:|
| sandbox | 369.81 | 21.78 | 568/590 | 2778/2778 | 0.00 | 10.1s |
| sieve | 318.55 | 75.70 | 808/1765 | 82/82 | 7326.36 | 49.5s |
| bands | 291.47 | 56.27 | 195/412 | 54/54 | 1345.57 | 18.5s |
| full-stack | 244.86 | 65.45 | 456/463 | 163/192 | 1425.63 | 18.5s |
| actual | 208.01 | 19.02 | 253/283 | 376/376 | 0.00 | 6.3s |
| chains | 105.74 | 20.44 | 384/384 | 5/5 | 0.00 | 5.6s |

Every run reports lifetime 30/30 — the whole-corpus confirmation of
search-01 finding 2 (the screen horizon saturates the lifetime term for
any persistent regime).

## Findings

### 1. The screen-horizon corpus is effectively free

The whole 11-run sweep took ~2.5 minutes; bands alone took 18.5 s here
versus 6 451 s at 20k (×350). The 20k cost lives almost entirely in bond
accumulation between tick 3k and tick 20k, not in the early dynamics.
Consequence: there is no excuse to skip the corpus gate before paying
for any 20k evaluation — and re-running this sweep after content changes
is cheap enough to do routinely.

### 2. The horizon reorders the leaderboard — the gate is a filter, not a verdict

At 20k the condensers dominate (actual 3 631, bands 2 712); at 3k they
place 5th and 3rd — condensation has not paid off yet, and sandbox tops
the table instead. So beating the corpus at 3k does **not** imply beating
it at 20k, and vice versa. The honest use of this sweep: a champion that
loses to the corpus at the screen horizon is not worth a 20k run; one
that wins here still has to survive the 20k comparison, which stays the
exit-criterion authority.

### 3. Search-01's champion, placed against the corpus at last

`g005-i004` (screen headline 331.77, fitness 76.44): **above every
corpus row on fitness** (max: sieve 75.70) — under the function the
search actually climbs, the discovered regime is already corpus-best at
the screen — but **second on the exit-criterion scalar** (sandbox:
369.81). Both statements hold at the screen horizon only (finding 2).
The gap to sandbox is structural: sandbox buys its headline with a
2 778-particle proto-blob, exactly the shape fitness discounts and the
scalar rewards.

### 4. Cross-build byte-identity held

The sieve row (`0xfef118b2a1bcc52a`) is byte-identical to the committed
record from build `d726376` (sieve-3k.score.ron, 2026-07-13), and the
chains row (`0x26485acd6f277a32`) matches search-01's chains seed screen
from build `414fc7e` — three builds, one platform, identical bits. As it
must be: the intervening commits touch tooling only, and determinism is
per-build-*content*; still, a passing cross-check is worth a line, and a
failing one would have been a fire alarm.

### 5. Sandbox's screen lead is early condensation

Sandbox at 3k already holds a largest structure of 2 778 (of 10 000
particles) on its way to 9 035/10 459 at 20k, with zero information.
Its fitness (21.78) is accordingly low — the structures term is its only
support. A search seeded near sandbox would inherit the amplifying-pack
runaway (its 20k row cost 556 s and 1–2 M bonds); anyone adding it to a
search corpus should size `bond_cap` for that class, per the search-01
finding-3 sizing rule.
