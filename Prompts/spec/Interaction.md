# Interaction
Purpose: Universal discrete state transition. The emergence engine.

Scope:
Discrete, rule-driven events (bonding, transfer, transformation).
Continuous dynamics (motion, forces, diffusion, heat) belong to Physics — see spec/Physics.md.

Format:
Condition
→ Probability
→ Cost
→ Action (transfers, bond changes, create/destroy)

Pipeline (per candidate interaction):
1. Match condition
2. Roll probability (chunk-local RNG stream)
3. Check costs payable — abort if not
4. Apply costs
5. Apply action: transfers, bond changes, particle create/destroy
6. Commit (two-phase: collect intents, commit deterministically)

Transfer:
- matter
- energy
- information

Create/Destroy:
Interactions may create and destroy particles (split, merge, emit, absorb).
Total matter + energy conserved across the event (physics mode).
Ids assigned sequentially, never reused.

Rules:
- Data-driven. Rules are content, not code.
- Rule set is part of replay identity (hash it into saves/replays).
- No hardcoded chemistry, reproduction, mutation, evolution.
- Particle memory/bonds/state mutate ONLY through interactions.

AI Checklist:
[ ] Deterministic RNG (split streams)
[ ] Stateless systems (all state in particles/world)
[ ] Extensible
[ ] Rule matching scales (indexed by condition, radius-capped)
