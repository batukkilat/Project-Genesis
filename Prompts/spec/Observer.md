# Observer
Purpose: Analyze simulation without affecting it.

Input:
Simulation snapshots (read-only). Runs on its own cadence, decoupled from the sim loop — never every tick at scale.

Output:
Hypotheses only.

Possible labels:
- Possible Life
- Possible Intelligence
- Possible Civilization
- Possible Technology
- Possible awareness of the external observer

Rules:
- Read-only
- Never modifies simulation
- Confidence-based
- Detection criteria are configurable metrics, not hardcoded definitions (open design work, Phase 5)

Replay compatible = running with Observer on or off produces identical simulation state hashes.

AI Checklist:
[ ] Separate module
[ ] Replay compatible
[ ] Deterministic given same snapshot + config
