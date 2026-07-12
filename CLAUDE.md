# Project Genesis — Claude instructions

Priority order: 1 Correctness, 2 Determinism, 3 Maintainability, 4 Performance, 5 Features.

Read before working:
- Prompts/MASTER_PROMPT.md — the constitution. Immutable rules. Wins all conflicts.
- ROADMAP.md — canonical phased plan.
- docs/PLAYBOOK.md — craft standards: replay-identity rules, decision process, testing bar. Cited precedents, not suggestions.
- Prompts/spec/ — per-system specs.

Rules:
- Generic systems > Earth-specific logic.
- Composition > inheritance.
- Simulation independent from UI/Renderer/AI.
- No biological or civilizational terms in code below the Observer layer.
- Test everything; determinism verified by state-hash tests.
- On WSL, build with CARGO_TARGET_DIR outside /mnt/c (drvfs I/O is slow).
