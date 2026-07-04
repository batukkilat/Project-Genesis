# GOAL — Continuous autonomous work directive

Advance Project Genesis through its roadmap continuously.

Sources of truth: Prompts/MASTER_PROMPT.md (constitution, wins all conflicts),
ROADMAP.md (phases + decisions log), Prompts/spec/*. Memory has project state.

## Work loop (repeat)

1. Pick the next unblocked item from the current phase in ROADMAP.md.
2. Before code: if the item needs a decision NOT in the decisions log
   (architecture fork, new player-facing behavior, replay-identity change),
   do NOT assume — write the question with options + recommendation into
   QUESTIONS.md, commit it, skip the item, and pick the next unblocked one.
   Ask the user directly (AskUserQuestion) only if every remaining item is
   blocked.
3. Implement with tests first-class: every sim change ships with determinism
   tests (replay, save/resume, thread-count invariance) and conservation
   tests where relevant. cargo test + clippy clean + cargo fmt before commit.
4. Gate: run `genesis verify` after physics/sim changes; update BASELINES.md
   when performance-relevant. A regression >20% needs a written reason.
5. When a phase's exit criteria all pass: mark it done in ROADMAP.md, bump
   workspace version, commit (conventional format), push to origin main.
   Also commit+push meaningful mid-phase checkpoints — never let work sit
   unpushed longer than a session.

## Delegation (use freely, Opus subagents)

- cavecrew-investigator: locate code / map call sites before refactors.
- cavecrew-builder: bounded 1-2 file mechanical edits (specs, docs, renames).
- cavecrew-reviewer: review the diff before every push; fix CONFIRMED
  findings before pushing.
- general-purpose: research tasks (e.g. bond-storage layouts, emergence
  metrics literature) parallel to main-thread implementation.

Parallelize only independent files/tasks; interdependent sim code stays in
the main thread.

## Hard rules

- Never violate the constitution; no biological terms below the Observer
  layer; simulation stays headless-runnable and AI-free; every outcome
  valid — no win/lose logic, ever.
- Build env: PATH includes ~/.cargo/bin and
  CARGO_TARGET_DIR=$HOME/.cache/genesis-target (project sits on /mnt/c).

## Pace and stop condition

Self-paced. Stop iterating when the current phase is done and all remaining
items are blocked on QUESTIONS.md — then summarize and wait for the user.
