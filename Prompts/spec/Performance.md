# Performance
Goals:
- Millions of particles
- Deterministic
- Stable memory
- Parallel friendly

Techniques:
- ECS
- Chunk simulation
- Adaptive detail (state-keyed, deterministic — never wall-clock-keyed)
- Spatial partitioning
- Cache-friendly data
- Profiling first

Known scaling risks (address in the phase that hits them):
- Rule matching: N particles × R rules × K neighbors blows up. Index rules by condition, cap interaction radius, evaluate chunk-locally, precompile matchers. (Phase 3)
- Bonds: particle graphs are cache-hostile. Storage design is an open question. (Phase 3)
- Observer cost: metrics over millions of particles cannot run every tick. Observer consumes snapshots on its own cadence, decoupled from the sim loop. (Phase 5)
- Rendering: never draw millions of sprites. Aggregate to density/field representations at low zoom. (Phase 6)

Never optimize by reducing correctness.

AI Checklist:
[ ] Benchmark
[ ] Profile
[ ] Avoid unnecessary allocation
