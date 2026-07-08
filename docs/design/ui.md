# UI — design notes for Phase 6

Status: design parked ahead of time (2026-07-08), companion to visuals.md
(the look; this file is the chrome and interaction design). Nothing here is
implemented; nothing here binds the simulation. The UI is a pure consumer of
snapshots and Observer output, and a producer of player actions — which are
replay-recorded events (constitution rule 6), never direct state edits.

Reference point the owner named: WorldBox / The Sandbox Evolution — top-down
pixel god-sandbox, toolbar of powers, time controls, no objectives. That is
the genre shape. The differences below are deliberate and follow from the
constitution, not from taste.

## Where we match the genre

- Top-down pixel planet, continuous zoom (visuals.md LOD tiers T0–T3).
- Bottom toolbar of god-powers with parameterized brushes.
- Time controls: pause / play / warp. Warp = more ticks per wall second,
  never a bigger dt (decisions log).
- Pure sandbox: no objectives, no fail state, no loading screens in the
  common path (Phase 6 exit criteria).

## Where we deliberately differ

1. **No creature dropper.** Player verbs are environment-only (Phase 4
   deliverables): temperature, pressure, radiation, tectonic events,
   rotation, magnetic field, asteroid impacts, time scale. The closest thing
   to spawning content is an asteroid payload — a quantity-range profile,
   never a named material (decisions log, 2026-07-06). Anything that looks
   alive must have emerged.
2. **Nothing has a name until the Observer earns it.** No labels on world
   objects. The Observer proposes hypotheses with confidence scores
   ("possibly self-replicating, 0.7"); the UI presents them as instrument
   readings, not facts. More microscope than minimap.
3. **Timeline branching.** Any run forks into an independent universe (save +
   player-action log + shared ancestry metadata). The branch tree is a
   first-class screen, not a save-slot list.

## Settled

- **Observer annotations are panel-only (owner, 2026-07-08).** No outlines,
  halos, or badges drawn on world objects. Drawing "this might be an
  organism" onto the world edits the player's perception of emergence — the
  strongest moment this game has is the player noticing something before the
  Observer does. Selecting a hypothesis in the panel may pan/zoom the camera
  to the structure's location; it still draws nothing on it. (Also recorded
  in the ROADMAP decisions log.)

## Screen layout (v1 proposal)

```
+------------------------------------------------------------------+
| time: pause/1x/warp   tick counter   sim speed      [menu] [obs] |
|                                                                  |
|                                                                  |
|                        world viewport                            |
|                 (zoom T0..T3, pan, click=inspect)      [observer |
|                                                         panel,  |
|                                                         slide-  |
|                                                         out]    |
|                                                                  |
| [env tool icons: temp | pressure | radiation | tectonic |        |
|  asteroid | rotation | mag field]        tool params strip      |
+------------------------------------------------------------------+
```

- **Toolbar (bottom):** one icon per environment tool. Selecting a tool shows
  its parameter strip (brush radius, intensity, sign) above the toolbar.
  Applying it emits a player action into the replay stream.
- **Time (top-left):** pause, 1x, warp presets. Warp shows achieved
  ticks/second so the player learns the machine's budget honestly.
- **Observer panel (right, slide-out):** hypotheses list (confidence-sorted),
  metrics (complexity, entropy, information density, persistence), history
  timeline. Hidden by default; the world is the point.
- **Inspector (click):** particle = raw quantities (matter/energy/
  information/velocity); at cluster zoom = aggregate stats for the structure
  under the cursor. Raw numbers, honestly labeled — no invented names.
- **Branch tree (menu screen):** saves shown as a tree by ancestry; fork,
  load, replay, share (seed + config + pack hash + action log).

## Principles

1. **UI reads snapshots, writes player actions.** Same contract as the
   renderer; there is no third path into the simulation.
2. **Chrome is honest.** Everything displayed is derived from state or
   Observer output. No flavor text below the AI narration layer (Phase 7),
   and narration is always attributed to the narrator, not the world.
3. **Asset budget follows visuals.md:** one CC0 UI kit (font, 9-slice
   panels, icons). Style from the render pipeline, not bespoke art.
4. **Keyboard-first parity later, mouse-first now.** Brushes and pan/zoom
   are pointer-shaped; don't design a second input scheme before the first
   one exists.

## Open questions (park until Phase 6 starts)

- Tool parameter UX: continuous sliders vs stepped presets (replay files
  serialize better with steps; sliders feel better — maybe stepped values
  under a continuous-feeling control).
- Whether the inspector needs a pinned/compare mode (two structures side by
  side) in v1 or later.
- Warp UI when the sim can't hit the requested rate (show target vs
  achieved, or auto-degrade silently — bias: show both, honesty principle).
- Whether the branch tree lives in-game or on the pause/menu layer.
