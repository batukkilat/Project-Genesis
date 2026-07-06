# QUESTIONS — decisions needed from the project owner

Design forks not settled in the ROADMAP decisions log. Work that depends on
an entry here is blocked until the answer is recorded in the decisions log;
everything else continues. Format: context, options, recommendation.

*(no open questions)*

**Standing guidance from the owner (2026-07-06):** when a parked question
carries a clear recommendation, autonomous sessions may adopt the
recommended option themselves — record it in the ROADMAP decisions log
(with the Q id) in the same commit, and only park here when no option is
clearly recommendable or the fork touches something irreversible.

## Resolved

- **Q-2026-07-06-A** (adaptive detail exactness) → Option A, recorded in
  the decisions log 2026-07-06. Phase 4 unblocked.
- **Q-2026-07-06-B** (information overflow) → Option A, information-only
  cap `information_max` in replay identity, recorded in the decisions log
  2026-07-06. **Implemented 2026-07-06** (save format v7): clamp at
  interaction commit, cap in replay identity, verified deterministic.
