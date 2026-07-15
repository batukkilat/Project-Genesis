# 5. The headless CLI, in full

`genesis-headless` is the research instrument: everything the engine can
do, runnable from a terminal with no window attached. The windowed app is
a view onto the same engine — same configs, same packs, same action
scripts, one representation — so everything you learn here transfers.

Build once and the binary lands at `target/release/genesis`; every
example below also works via `cargo run -p genesis-headless --release --`
in place of the binary path.

```sh
cargo build -p genesis-headless --release
```

## `run` — tick a world

```sh
target/release/genesis run --rules packs/chains.ron --ticks 5000
```

Creates a world (default config unless `--config` is given), loads a rule
pack (`--rules`; physics only when omitted), optionally feeds a player
action script (`--actions`), ticks it, and prints the final state hash —
the world's fingerprint. Useful flags:

- `--save world.gens` / `--load world.gens` — save at the end, or resume
  a saved world instead of creating one. A resumed run continues exactly
  where the save left off (`--rules` is refused with `--load`: the rule
  set is part of the save's identity).
- `--report N` — every N ticks, print one observer sample line
  (structures, persistence, hypotheses, quantity totals — see
  [chapter 9](09-observer.md)).
- `--timeline out.ron` — write the full observer timeline at the end
  (requires `--report`).
- `--observer my-observer.ron` — tune the Observer; never affects the
  simulation.
- `--hash-every N` — print intermediate state hashes, for comparing two
  runs' divergence point.
- `--threads K` — worker threads (0 = all cores). Never changes results,
  only speed.

## `verify` — prove a world is deterministic

```sh
target/release/genesis verify --rules packs/chains.ron --ticks 1000
target/release/genesis verify --config configs/env-gradient.ron --rules packs/bands.ron \
    --actions scripts/terraform-west.ron --ticks 3000
```

Runs the same world four ways — twice fresh, once through a mid-run
save/resume, once single-threaded — and demands one identical final
hash; prints `DETERMINISTIC over N ticks` and exits zero on success,
non-zero on divergence. This is the project's determinism contract as an
executable. If you author a config, pack, or script, run `verify` on it
before trusting anything you observe.

## `score`, `sweep` — measure worlds

```sh
target/release/genesis score --rules packs/chains.ron --ticks 20000 --every 100 --out chains.score.ron
target/release/genesis sweep --spec sweeps/shipped-packs-3k.ron --out results/
```

`score` runs a world, samples the Observer on a cadence (`--every`), and
collapses the timeline into one flat RON record: final and peak
structure counts, sizes, lifetimes, complexity, retained information,
hypothesis tallies, and the headline persistence × complexity scalar.
The record stamps seed, ticks, cadence, and the final state hash, so any
score is reproducible and any two scores at the same cadence are
comparable. `sweep` batches score runs from a spec file and adds a
`table.md` sorted by headline (batch order can't affect anything);
`sweeps/shipped-packs-3k.ron` is the shipped corpus at the cheap
3 000-tick gate and makes a good template.

## `mutate`, `search` — explore world-space

```sh
target/release/genesis mutate --rules packs/chains.ron --seed 1 --steps 2 --out mutants/
target/release/genesis search --spec sweeps/search-03.ron --out my-search/
```

`mutate` applies N schema-bounded mutations (parameter jitter, rule
drop/duplicate, condition rewire) to a (config, pack) pair and writes the
mutant plus an ancestry record listing every operation. `search` runs
the full evolutionary loop from a spec (seeds → screen → select → mutate
→ confirm); its output directory is a self-contained artifact — every
individual's authoring files, ancestry, scores, a leaderboard, and a
summary. Both are pure functions of (spec/seed, build): re-running
reproduces every file byte-for-byte, and any child in a search can be
re-derived by hand with `mutate` from its parent's files. The research
docs under [docs/research/sweeps/](../../docs/research/sweeps/) are the
worked examples.

## `branch` — fork a timeline

```sh
target/release/genesis branch --from world.gens --to fork.gens
```

Copies a save into an independent branch and writes an ancestry sidecar
(`fork.gens.branch.ron` unless `--record` says otherwise) noting the
parent, its state hash, and the fork tick. The child inherits the exact
state — pending actions included — and diverges only through what you do
next; the parent is untouched, and re-running it stays bit-identical.
This is the fork-and-compare workflow: one past, several futures, every
difference attributable. Branching never enters the engine — the sidecar
is bookkeeping above it. See chapter 8 for the full workflow.

## `bench` — measure throughput

```sh
target/release/genesis bench --particles 1000000 --ticks 120
target/release/genesis bench --particles 1000000 --ticks 120 --threads 1
target/release/genesis bench --config configs/lod.ron --no-lod
```

Prints ticks/s and particle-ticks/s (plus the state hash, so you can see
determinism hold across thread counts while speed changes). `--no-lod`
forces the LOD policy off so one config can be benchmarked LOD-on vs
LOD-off on the identical world. Reference numbers with machine, tick
count, and command live in [BASELINES.md](../../BASELINES.md) — don't
compare your numbers to anyone else's without comparing machines first.

## `init-config`, `init-rules` — authoring starting points

```sh
target/release/genesis init-config my-world.ron
target/release/genesis init-rules my-pack.ron
```

Write a commented default config / example rule pack to edit. Chapter 6
walks through every knob; the fastest path is to copy something from
[configs/](../../configs/) or [packs/](../../packs/) that already does
nearly what you want.

## Technical notes

- **Exit codes**: `verify` is the only subcommand whose success depends
  on an outcome (non-zero on divergence); everything else fails only on
  bad input (unreadable files, invalid RON, schema violations — the
  error message names the field).
- **Replay identity vs. reporting knobs**: `--config`, `--rules`,
  `--actions`, and the seed inside the config change the universe.
  `--threads`, `--report`, `--observer`, `--timeline`, `--every`,
  `--out` never can — a run with different values for these produces the
  identical state hash. `score` stamps its cadence into the record not
  because it changes the world but because it changes what the record
  aggregates.
- **Saves are self-describing and version-locked**: a `.gens` file
  carries its format version and an integrity hash; loading a corrupt or
  incompatible file is an error, never a silently different world.
- **`--particles` on `bench` overrides the config's count**; everything
  else about the world comes from `--config` as usual.
- **Logging**: set `RUST_LOG` (e.g. `RUST_LOG=warn`) to quiet or
  verbosify the tracing output; it never affects results.
