# Rendering
Purpose: Visualize simulation only.

Style:
- Retro pixel art
- Top-down
- Continuous zoom (planet -> particle); bounded, not literally infinite
- Renderer never changes simulation

Requirements:
- Read-only access (snapshots)
- LOD rendering: aggregate particles into density/field visuals at low zoom — never draw millions of sprites
- Chunk streaming
- Debug overlays
- Simulation speed independent

AI Checklist:
[ ] Rendering separated
[ ] Headless compatible
