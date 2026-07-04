# Bond Storage for Phase 3 (Interaction System)

Persistent pairwise links with per-bond scalar state, created/destroyed by
data-driven rules. Millions of particles, ~0.1–2 bonds/particle typical, no
hard per-particle cap. Constraints: determinism (all iteration/mutation order a
pure function of state), particles re-sorted into (cell, id) order every tick,
IDs stable and never reused, indices change every tick.

## TL;DR — Recommendation

Use a **hybrid: a canonical SoA edge list as the source of truth, plus a
per-tick CSR adjacency mirror rebuilt during canonicalization.**

- Edge list: parallel `Vec`s `(a, b, strength, …)` with `a < b` (endpoints are
  stable particle **IDs**), sorted canonically by `(a, b)` each tick. This is
  the save/replay state — cheap dedup, cheap append-then-sort insert.
- CSR mirror: directed **both ways**, keyed by post-canonical particle **index**.
  This is derived, not saved. It is what the force/interaction passes iterate.

Why not edge-list-only: a force pass that iterates edges must write forces to
*both* endpoints, i.e. two arbitrary particle slots per edge. That breaks the
existing "each parallel task writes only its own particle's slot" rule and would
force atomics or a reduction — non-deterministic order or contention. The CSR
mirror restores particle-major, disjoint-write iteration identical to the
current `forces()` pass (which already computes each pair from both sides, so
storing each bond twice matches the existing convention exactly).

Why not fixed slots: a hard cap is disallowed, and this is the only candidate
that inherently imposes one.

The CSR build is the **same counting-sort pattern already used for `cell_start`**
in `store.rs` — offsets by endpoint index, O(E), trivially parallel, fully
deterministic. Minimal new machinery.

## Per-candidate tradeoffs

### (a) Canonical SoA edge list, sorted by (min_id, max_id)
- **Cache (force pass):** endpoints are gathered via id→index into `px/py`
  sorted by cell — two scattered reads per edge. E ≈ N–2N, tolerable, but no
  locality.
- **Insert/delete (two-phase):** excellent. Collect create-intents, append,
  re-sort into canonical order (already re-sorting every tick). Delete =
  mark-and-compact or drop-on-rebuild. Dedup is a linear scan of the sorted list.
- **Destroyed particle:** dangling edges must be pruned; needs a liveness test.
- **Fatal flaw alone:** edge-parallel force accumulation writes both endpoints →
  violates the disjoint-write determinism model. Good as *storage*, bad as the
  *iteration structure*.

### (b) Fixed per-particle slots (LAMMPS `bondperatom` style)
- **Cache:** best — partners stored inline, move with the particle during the
  permute. Particle-major, disjoint writes.
- **Insert/delete:** cheap within a slot, but variable-length rows make the
  canonicalize permute awkward and the slots must ride along in every re-sort.
- **Destroyed particle:** partner still holds a stale slot; needs back-scan.
- **Rejected:** requires a hard cap (or a ragged/smallvec that reintroduces the
  variable-length permute cost). No-cap requirement kills it.

### (c) CSR adjacency rebuilt per tick from the edge list  — RECOMMENDED (with a)
- **Cache:** excellent. Particle i's bonds are a contiguous `csr_start[i]..
  csr_start[i+1]` slice — same access shape as `cell_start`. Iteration is
  particle-major with disjoint writes: drops straight into the existing parallel
  force/interaction model, deterministic for any thread count.
- **Rebuild cost:** O(E) counting sort keyed by endpoint index; parallelizable;
  reuses the `cell_start` prefix-sum idiom. Store each bond twice (once per
  endpoint direction).
- **Insert/delete:** happen on the edge list (source of truth); CSR is discarded
  and rebuilt, so no in-place CSR mutation is ever needed.
- **Destroyed particle:** handled at rebuild time (see below) — an edge whose
  endpoint id no longer maps is simply dropped.

### (d) MD-literature check
- **LAMMPS:** per-atom fixed cap (`bond_atom[i][k]` holds partner *global IDs*)
  plus an `atom->map()` (array or hash) translating global ID → local index,
  (re)built on neighbor-list steps. Confirms the id-based storage + per-reneighbor
  index map pattern; the cap is a LAMMPS limitation we deliberately avoid.
- **GROMACS:** keeps a global bonded list and *redistributes* it every
  domain-decomposition (reneighbor) step — i.e. rebuilds the local interaction
  lists from a stable master list exactly when the spatial sort changes. This is
  precisely the "master edge list + per-tick rebuilt CSR" split recommended here.

## id → index translation (the crux)

Bonds store stable **IDs**; passes need **indices**, and indices are scrambled
by the `(cell, id)` re-sort every tick. Options:

1. **Array `idx_of_id[id]`** — O(1), but IDs grow forever and are never reused,
   so the array bloats without bound and fills with holes. Rejected for Genesis.
2. **Hash map `FxHashMap<u64, u32>`, rebuilt each tick** — LAMMPS's "hash" map
   mode. Populate by iterating `id[]` once after `canonicalize` (O(N) inserts);
   look up each edge endpoint during CSR build (O(E)). **Determinism is safe
   because the map is lookup-only and never iterated** — output order comes from
   the canonically-sorted edge list and the index-keyed CSR, not from hash order.
   Recommended default.
3. **Sort + merge-join** — build `(id, index)` pairs, join against edge
   endpoints. Avoids hashing and has deterministic *performance*, but is more
   code and an extra O(N log N) pass. Fall back to this only if the hash map's
   cache misses show up in profiling.

**Destroyed-particle cleanup falls out for free from the lookup:** during the
per-tick CSR rebuild, if either endpoint's `idx_of_id` lookup misses, the
particle is gone — drop that edge from the master edge list (lazy prune) and
skip it. No separate liveness structure, no dangling references. Destruction
rules may *also* emit explicit bond-remove intents (CSR gives a particle's bonds
cheaply), but the lazy prune is the safety net that keeps the edge list
self-healing and deterministic.

## Recommended Rust sketch (fields only)

```rust
/// Source of truth for bonds. Sorted canonically by (a, b) every tick, so the
/// layout is a pure function of state → save/replay/resume stay bit-identical.
#[derive(Resource, Default)]
pub struct BondStore {
    // --- Saved state: SoA edge list. Endpoints are stable particle IDs, a < b.
    pub a: Vec<u64>,          // lower endpoint id
    pub b: Vec<u64>,          // higher endpoint id
    pub strength: Vec<f32>,   // per-bond scalar state (add more SoA columns here)

    // --- Derived, rebuilt each tick after canonicalize; NOT saved.
    // CSR adjacency, directed both ways, keyed by post-canonical particle index.
    csr_start: Vec<u32>,      // len = n_particles + 1 (prefix-sum, like cell_start)
    csr_partner: Vec<u32>,    // partner particle INDEX
    csr_bond: Vec<u32>,       // back-ref row into the edge list (strength, …)

    // id → current index. Rebuilt each tick; lookup-only, never iterated.
    idx_of_id: rustc_hash::FxHashMap<u64, u32>,

    // --- Two-phase commit scratch (collect intents, commit deterministically).
    create: Vec<(u64, u64, f32)>, // (a, b, initial strength)
    destroy: Vec<u32>,            // edge-list rows to remove
}
```

## References
- LAMMPS `atom_modify` / `Atom::map()` — global-ID→local-index map (array vs
  hash), created for molecular systems with permanent bonds:
  https://docs.lammps.org/atom_modify.html ,
  https://docs.lammps.org/Developer_atom.html
- GROMACS domain decomposition — bonded lists redistributed each neighbor-search
  step from a stable global list:
  https://manual.gromacs.org/current/reference-manual/algorithms/parallelization-domain-decomp.html
- GROMACS bonded interactions (fixed atom lists):
  https://manual.gromacs.org/current/reference-manual/functions/bonded-interactions.html
