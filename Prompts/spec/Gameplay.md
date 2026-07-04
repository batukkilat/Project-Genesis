# Gameplay
Loop:
Observe
→ Modify environment
→ Simulate
→ Discover
→ Repeat

Player can:
- Change planetary parameters
- Pause
- Time warp (runs more ticks per wall second; NEVER changes dt — determinism survives)
- Branch timeline (fork = copy of save + player-action log; branches are independent saves sharing ancestry metadata)
- Inspect data

Player cannot:
- Spawn organisms
- Edit organisms
- Force evolution

Player actions are recorded in the replay stream — they are part of replay identity.

Win Condition:
None.

Outcome:
Discovery.
