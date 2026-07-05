# Visuals — design notes for Phase 6

Status: design parked ahead of time (2026-07-05). Nothing here is
implemented; nothing here binds the simulation. The renderer is a pure
snapshot consumer (constitution; spec/Rendering.md) — every idea below is a
*read* of simulation state, never a hook into it.

## Principles

1. **Animation is the simulation.** No sprite animation, no rigs, no walk
   cycles. Motion on screen is interpolation between snapshots. Anything
   that "moves" moves because physics moved it. There is nothing to animate
   by hand, because nothing below the Observer layer has a name.
2. **Visuals are data-driven mappings, not assets.** A particle's look is a
   pure function of its quantities; a structure's look is the sum of its
   particles and bonds. Rule packs change the universe; the same mapping
   then shows a different universe without any new art.
3. **Asset budget is deliberately tiny.** A soft-dot sprite, a few palettes,
   one UI kit, shaders. No character art ever.
4. **The mapping is content.** Like rule packs, palettes/mappings are data
   files a player could swap. They are NOT part of replay identity — they
   change how the world looks, never what happens.

## Quantity → visual mapping (v1 proposal)

Per particle, at sprite-level zoom:

| Channel | Drives | Rationale |
|---|---|---|
| matter | sprite radius (sqrt scale) + base value (darker = heavier) | mass reads as size/weight |
| energy | brightness/emissive glow | hot things glow; an energetic region reads instantly |
| information | hue shift along a palette ramp | the "interesting" quantity gets the most salient channel |
| velocity | optional short motion streak (debug overlay first) | cheap readability of flow |

Bonds: 1px line between endpoints, alpha = strength, color = average of
endpoint colors. Bonded clusters then read as solid bodies for free — the
emergent structure outlines itself.

Events (create/destroy, once Phase 3 lands them): brief 2–3 frame
flash/pop decal at the event position. The only "animation asset" in the
game, and it is procedural.

## Zoom LOD tiers (bounded continuous zoom, planet → particle)

Never draw millions of sprites (spec/Rendering.md). Tiers by
particles-per-screen-pixel, not by camera math:

| Tier | Roughly | Draw |
|---|---|---|
| T0 particle | < 1 particle per pixel | sprites + bond lines + event decals |
| T1 cluster | 1–100 per pixel | point cloud, no bonds; bonds implied by proximity |
| T2 field | 100–10k per pixel | density/quantity heatmap from the spatial grid (cell aggregates: count, mean energy, mean info) |
| T3 planet | whole world on screen | same heatmap, coarser mip + torus-aware wrap; environment gradients (Phase 4) underneath |

The sim already maintains the uniform grid; T2/T3 are one aggregation pass
over `cell_start` — no new simulation machinery. Aggregation happens
renderer-side from the snapshot.

Retro pixel look: render to a low-res offscreen target (e.g. 480×270),
integer-upscale, ordered dithering on the heatmap tiers. Style comes from
the pipeline, not from hand-pixeled art.

## Asset shortlist (complete list, intentionally short)

- 1 soft-dot particle sprite (plus 1 "core" variant), hand-made, minutes of
  work.
- 3–5 palette ramps (data files): default, colorblind-safe, debug.
- 1 CC0 UI kit (font + 9-slice panels + icons; e.g. Kenney) for
  inspect/pause/warp/branch chrome.
- Shaders: heatmap, dither, glow, bond lines — code, not assets.
- Audio: out of scope until Phase 6+; candidate approach is data-driven
  sonification (event rate → texture), same philosophy as visuals.

## Debug overlays (build first, they de-risk the rest)

Grid cells, cell occupancy, bond graph, per-quantity single-channel view,
interaction-event markers. These are the mapping system minus aesthetics —
shipping them first validates snapshot→screen throughput before any styling.

## Open questions (park until Phase 6 starts)

- Interpolation vs sim-locked frames when time warp outruns render rate
  (likely: render latest snapshot, interpolate only at 1× speed).
- Whether Observer annotations (Phase 5 hypotheses) render as outlines or
  stay UI-panel-only. Bias: panel-only first — drawing outlines around
  "possible organisms" edits the player's perception of emergence.
- Palette/mapping file format (probably RON, same loader idioms as packs).
