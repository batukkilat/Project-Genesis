# 9. The Observer

Run a world long enough and things start to *look like* something:
clusters that keep their shape, chains that grow, knots that survive
churn. The Observer is the layer that notices — a scientist watching
your universe through one-way glass. It finds structures, follows them
through time under stable names, measures them, and offers carefully
hedged guesses about what they might be. It never touches anything: the
same world runs bit-identically whether the Observer watches or not,
and that is a tested guarantee, not a promise.

## Watching a run

Ask any headless run for periodic reports:

```sh
cargo run -p genesis-headless --release -- run --rules packs/chains.ron --ticks 1000 --report 500
```

Each report line is one observation sample:

```text
tick 500  n 10000  bonds 244  comps 218 (largest 4, in-multi 55)  persist>=5 0 (oldest 1)  hyp self-maint 0 grow 0  M 5478.628  E 4993.539  I 0.000
```

Reading left to right: the tick; particle and bond counts; how many
bonded structures exist (`comps`), the largest one's member count, and
how many particles sit in multi-particle structures; how many structures
have persisted at least 5 samples and the age of the oldest; how many
structures currently carry each hypothesis; and the world totals of
matter, energy, and information. Watching `persist>=` and `oldest` climb
while `comps` stays stable is the signature of a world that is building
something rather than just boiling.

To keep the full record instead of console lines, add `--timeline`:

```sh
cargo run -p genesis-headless --release -- run --rules packs/chains.ron --ticks 1000 \
    --report 100 --timeline chains.tl.ron
```

The timeline is a RON list with one entry per sample: aggregate stats,
every tracked structure's metrics, and any hypotheses. It is the raw
material for your own analysis — the repo's research docs are built from
exactly these dumps (see
[docs/research/sweeps/](../../docs/research/sweeps/)).

In the windowed app, the same information backs the inspector: click a
particle to see its structure, focus the camera on a structure, and read
its metrics in the panel. Observer annotations live in panels only — the
world view never draws labels on your universe.

## What the Observer measures

Every tracked structure gets, per sample:

- **persistence** — consecutive samples this structure has kept its
  identity. The Observer matches structures between samples by member
  overlap, so a structure can lose and gain members and still be *the
  same one*, the way a river is the same river.
- **stability** — how much of its membership it kept since the last
  sample (1.0 = frozen, lower = churning).
- **complexity** — one number for "how organized": the sum of a size
  term, a heterogeneity term (do members play different roles, measured
  on their bond counts?), and a connectivity term (how bonded is it?).
  A chain, a ring, and a dense blob of the same size score differently,
  which is the point. The three terms are also reported separately
  (`degree_entropy`, `mean_degree`) so you can see *which* one carries a
  score.
- **information** — the total information its members currently hold:
  does the structure retain signal, or leak it?

## Hypotheses, not verdicts

On top of the measurements sit exactly two claims, each scored with a
confidence in [0, 1] and both deliberately prefixed "possibly":

- **possibly self-maintaining** — old enough, and its membership stayed
  stable across the recent window. Something is holding it together.
- **possibly growing** — present through the whole window, never shrank,
  ended bigger.

Absence of a hypothesis means "nothing to report", never "refuted". The
Observer only ever makes positive, hedged claims — labels like "alive"
or "intelligent" do not exist anywhere yet, because no current
measurement could honestly support them. (The 2026-07-14 search findings
include a cautionary tale: *possibly growing* fires at full confidence
on a structure that merely accumulates and never renews itself — a
hypothesis is a hint about where to look, not a conclusion.)

## Technical notes

- **Read-only by construction.** `genesis-observer` consumes snapshots;
  its API takes no mutable simulation references. The replay-identity
  test runs the same world with observation on and off and requires
  identical state hashes at both library and CLI level.
- **Never replay identity.** Observer settings change what is
  *reported*, never what *happens* — they are not hashed, not saved into
  `.gens` files, and changing them mid-experiment is always safe.
- **Stable ids.** A structure's observer id is assigned once and never
  reused. Identity continuation uses a configurable overlap threshold;
  tune it with a RON config passed via `--observer`:

  ```sh
  cargo run -p genesis-headless --release -- run --rules packs/chains.ron \
      --ticks 1000 --report 100 --observer my-observer.ron
  ```

  ```ron
  // my-observer.ron — defaults shown; every field optional.
  (
      overlap: 0.5,                    // member share to continue an identity, (0,1]
      persist_after: 5,                // samples before "persistent" in reports
      window: 5,                       // samples each hypothesis examines (>= 2)
      self_maintaining_age: 10,        // min age before self-maintaining is entertained
      self_maintaining_stability: 0.75 // per-sample stability the window must hold
  )
  ```

- **Metric definitions** (per structure, per sample): persistence = age
  in samples; stability = Jaccard similarity of consecutive memberships
  (newborn = 1.0); complexity = `ln(size) + H + ln(1 + mean_degree)`
  where `H` is the Shannon entropy (nats) of the member bond-degree
  histogram — `degree_entropy` and `mean_degree` are reported alongside
  and always reconstruct the scalar exactly; information = sum of member
  information.
- **Hypothesis scoring**: self-maintaining confidence = age ramp
  `min(1, persistence / (2 · self_maintaining_age))` × minimum window
  stability; growing confidence = strict growth steps / (window − 1).
- **Sampling cadence matters.** Persistence is counted in *samples*, so
  `--report 100` and `--report 500` aggregate different timelines from
  the same world. Comparisons are only meaningful at one cadence — the
  scoring tools stamp it into every record for exactly this reason.
- **Scores.** `genesis score` collapses a timeline into one flat RON
  record (final + peak aggregates and the headline
  persistence × complexity scalar); `genesis sweep` batches score runs
  and writes a comparison table; `genesis search` runs an evolutionary
  loop over worlds using Observer-derived fitness. All Observer output,
  never inputs: nothing they compute can reach back into a simulation.
