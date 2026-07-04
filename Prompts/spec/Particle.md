# Particle
Purpose: Only primitive simulation entity.

Fields (full set; each introduced when its governing system lands):
- id            Phase 1. Sequential u64, never reused.
- position      Phase 1. 2D continuous (f32), torus world.
- velocity      Phase 1.
- matter        Phase 1. Fundamental quantity.
- energy        Phase 1. Fundamental quantity.
- information   Phase 1 (scalar carrier); semantics defined Phase 3.
- memory        Phase 3. Semantics defined with interaction system.
- bonds         Phase 3. Storage design open (cache-hostile graph risk).
- state         Phase 3.

Rules:
- No biology.
- No species.
- No inheritance.
- Mutated only by simulation systems: Physics (position/velocity/energy) and Interactions (everything). Never by renderer, Observer, AI, or player.
- Created/destroyed only by interactions (after initial spawn). Conservation holds across events.
- Serializable.
- Deterministic.

AI Checklist:
[ ] Generic
[ ] ECS-friendly
[ ] Cache-friendly
[ ] Testable
