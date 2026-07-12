# User guide — plan for the Phase 6 guidebook

Status: plan parked 2026-07-12, companion to ui.md (the chrome) and
visuals.md (the look). This file plans the **player/researcher-facing
guide** — how a person installs, runs, plays, authors, and reproduces —
not the contributor docs (README/ROADMAP/PLAYBOOK already cover those).
Writing starts after the owner's Windows GPU test settles two open
presentation questions (§ Blocked below); the chapter skeleton and
sourcing can be judged now.

## Shape

- **Deliverable:** `docs/guide/` — one file per chapter, numbered, with a
  `docs/guide/README.md` table of contents. Chapters over one monolith:
  the app HUD / a menu screen can deep-link a chapter, chapters gate on
  different features (half are writable today, half wait on step 5 UI),
  and diffs stay reviewable.
- **Audience split, not document split:** every chapter opens with the
  play-facing path (what to click/press, what you'll see) and closes with
  a *Technical notes* section (flags, schemas, determinism implications).
  One document serves both readers; nobody maintains two.
- **Every command copy-paste runnable** against a fresh clone — same bar
  as README's quick start. Shipped configs/packs/scripts are the example
  corpus (they're already rot-guarded by tests, `d245798`).
- **Docs move with the feature** (PLAYBOOK §6): once a chapter exists,
  the commit that changes a flag or keybinding updates the chapter.

## Chapter plan

1. **What Genesis is** — particles + interactions only, everything else
   emerges; you shape environments, never creatures (ui.md "no creature
   dropper"); the layer model (sim → observer → render → AI) in one
   diagram; what "deterministic sandbox" buys you (reproduce, share,
   fork). Source: MASTER_PROMPT.md, distilled and de-jargoned.
2. **Install & run** — three paths with the same checkout: Windows
   native (`tools/run-app.ps1`, winget one-time setup), WSL/WSLg
   (llvmpipe caveats, `--fps`, `LP_NUM_THREADS`), Linux. System
   requirements table by reference to README. Troubleshooting appendix
   candidates: MSVC/link.exe, libxkbcommon on minimal X11 boxes,
   OneDrive vs target dir, drvfs build speed.
3. **First world (tutorial)** — guided 10 minutes in `genesis-app`:
   launch default config, read the HUD (tick, target-vs-achieved
   ticks/s, starvation flag, tier), zoom T3 heatmap → T0 particles+bonds,
   pause/warp, paint a field gradient with the brush, watch bands form
   with `configs/env-gradient.ron` + `packs/bands.ron`. Ends with "what
   you just did was replay-recorded" as the hook into ch. 7.
4. **The windowed app, in full** — `genesis-app` reference: every CLI
   flag (--config/--rules/--actions/--mapping/--palette/--zoom/--fps/
   --smoke), every binding (wheel zoom, WASD/right-drag pan, space,
   1–4 warp presets, left-drag brush), zoom-tier semantics (T0–T3,
   particles-per-pixel thresholds), warp honesty (target vs achieved,
   why dt never changes), visual mappings and palettes as swappable RON
   (never replay identity). Grows sections as step 5 lands: panels,
   save/load/branch UI, fork button.
5. **The headless CLI, in full** — `genesis-headless` reference: run
   (--report, --observer, --timeline, --save/--load), verify (what the
   four-way check proves), bench (reading BASELINES.md numbers), branch,
   init-config/init-rules as authoring starting points. Frame: headless
   is the research instrument; the app is the window onto the same
   engine — one representation, both documented against the same flags.
6. **Authoring worlds** — config RON walkthrough (physics incl. spin,
   env fields + dynamics, LOD policy — each knob: what it does, whether
   it's replay identity), rule packs (condition → probability → action;
   guided tour of shipped packs from `diffusion` to the `actual` vs
   `sandbox` two-regime pair as worked examples), validation errors and
   what they mean. The schema tables generate-or-check against
   `genesis-config` doc comments so they can't silently rot.
7. **Player actions, scripts, replay** — the action vocabulary (field
   ops, asteroid impacts, tectonic rifts, `SpinSet`), tick-stamped
   scripts (`scripts/` tour: terraform-west, bombardment, rift, spin-up,
   full-stack), the one-representation guarantee (live brush ≡ script ≡
   recording, Q-2026-07-08-B), how to re-run and share a session
   (seed + config + rules + actions = the run).
8. **Saves, forks, timelines** — GENS saves (self-describing, version-
   locked, integrity-hashed — what "load error" protects you from),
   save/resume mid-anything, `genesis branch` + ancestry records, the
   fork-and-compare research workflow (run overnight headless, open the
   save in the app in the morning — the loop the owner named).
9. **The Observer** — structures with stable ids, metrics v1
   (persistence/stability/complexity/information), hypotheses as
   confidence-scored *suggestions* (the observer never affects the sim
   — provably hash-identical on/off), reading `--report` and
   `--timeline` output, inspector picking/focus in the app.
10. **Performance & scale** — what to expect at 10k/100k/1M/10M
    (BASELINES.md by reference, never duplicated numbers), threads, LOD
    policy tradeoffs, software-rendering fallback behavior, why warp is
    CPU-bound and what "starved" means.

## Blocked on the Windows test (owner, tomorrow)

Two presentation questions the GPU session decides; they gate chapters
2–4's final form but not their drafting:

- **Menu screen or not.** If a proper menu/launcher lands, ch. 2–3
  route through it and CLI flags demote to Technical notes; if the app
  stays launch-into-world (current shape, matches "no loading screens
  in the common path"), the flag table stays primary. Don't write the
  onboarding chapter twice — draft ch. 3 last.
- **Real-GPU visual polish** may change tier thresholds/defaults;
  screenshots (if any) wait for real-GPU frames. Prefer text + ASCII
  HUD sketches over screenshots where possible — they diff and don't
  rot.

## Writable today (no blockers)

Chapters 1, 5, 6, 7, 8, 9, 10 depend only on shipped, pushed behavior —
a cloud shift can draft them against origin/main (headless-verifiable,
PLAYBOOK §5 boundary). Chapters 2–4 need the Windows verdict and the
step 5 UI half.

## Acceptance bar

- A fresh user on a fresh Windows machine reaches "watched my first
  world, painted a field, saved it" using ch. 2–3 alone, without
  reading source or asking.
- Every command in every chapter runs verbatim from repo root.
- Each replay-identity-relevant knob is explicitly marked "changes the
  universe" or "cosmetic" — the guide is where users learn the
  determinism contract without reading PLAYBOOK.
- Guide chapters are listed in README so they're discoverable from the
  repo front page.
