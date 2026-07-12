# SHIFTLOG — what each shift did, proven how, and one thing worth learning

Newest first. Format defined in GOAL.md (§ Shift log). Written for the
owner: full technical vocabulary, explained rather than simplified.

---

## 2026-07-12/13 — desktop session (Fable)

Commits pushed: `06417bb` rotation as frame spin (save v15), `51f1ba9`
windowed app shell, `c312be6` fps cap, `5cd4ec1` Windows launcher,
`5874a6e` system requirements, `6b33483` PLAYBOOK, guide plan, Phase 6.5.

**What changed and why.** Two big things. First, planet rotation entered
the physics as *frame spin*: instead of moving the world, we work in a
rotating reference frame, where every particle feels a Coriolis
deflection — a force perpendicular to its velocity, proportional to speed
and to the spin rate Ω. This is the "f-plane" approximation from
geophysical fluid dynamics: the standard way rotation enters a flat 2D
domain. It was implemented not as a force added to the integrator but as
an *exact rotation of each velocity vector* by angle −2Ω·dt each tick.
Second, the windowed app (`genesis-app`) landed: a Bevy shell that owns
the simulation, runs whole ticks per frame via the warp pacer, and draws
the extraction output (sprites+bonds zoomed in, heatmaps zoomed out).

**How it was proven.** Energy conservation test (Coriolis force is always
perpendicular to motion, so it can do no work — kinetic energy must come
out bit-identical, and does); momentum-magnitude test (total momentum
rotates under spin but |P| stays constant — the invariant moved, it didn't
vanish); replay-identity tests (zero spin hashes exactly like the
pre-feature build; nonzero spin is a different universe from tick 0);
save/resume mid-spin; thread-count invariance; `genesis verify` four-way
on scripts/spin-up.ron. What these cannot catch: whether the *emergent
consequences* of spin (chiral drift, band formation) are physically
sensible — that needs long runs and eyes, not unit tests.

**What to watch.** The app was only verified under software rendering
(WSLg/llvmpipe); real-GPU behavior — tier thresholds, texture upload
performance — is unconfirmed until the Windows test. The wgpu "gles"
feature shim in genesis-render's Cargo.toml exists only to force a
backend into the shared wgpu build; if bevy ever forwards that feature
itself, the shim becomes redundant and should be removed.

**Concept of the shift: why an exact rotation instead of a force.** The
naive way to apply Coriolis is explicit Euler: `v += a·dt` each tick. But
that formula moves the velocity along a *tangent line* to the circle it
should be tracing, so every tick the speed grows slightly — energy is
injected from nothing, forever, and the error compounds. The alternative:
since Coriolis only ever *turns* velocity without changing its length,
apply the turn exactly — multiply the velocity by a rotation matrix for
the precise angle covered in one tick. Speed is preserved to the last
bit, and the method is stable at any dt. Plasma physicists call this the
Boris pusher. The general lesson (PLAYBOOK §3): when a continuous law
conserves a quantity, choose the discrete formula that conserves it
*exactly*, then test the invariant — never settle for "small error per
tick," because ticks are the one thing this project has millions of.
