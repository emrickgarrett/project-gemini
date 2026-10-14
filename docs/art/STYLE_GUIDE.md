# Project Gemini: Art Style Guide v0.1

## 1. Pillars
- **Toy-bright 90s action.** Chunky low-poly shapes and flat saturated vertex color. Glow marks anything that matters.
- **Readable chaos.** With 300 bugs and 8 players on screen, the player can still tell friend from foe and danger from scenery at a glance.
- **Our own IP.** Jet Force Gemini is a mood reference only. No copying of names, characters, silhouettes or assets.

## 2. Color
| Role | Colors | Notes |
|---|---|---|
| Players | Teal, cobalt, white, plus 1 accent per hero | Cool colors = friendly |
| Bugs | Acid green, amber, magenta glow | Warm/toxic colors = hostile |
| Environment | Mid-value, desaturated | Never brighter than actors |
| Rarity | Common silver, rare blue, epic purple, legendary gold | Same everywhere: crates, upgrades, UI |

Emissive and bloom are used only on: projectiles, weak points, pickups, rarity glows, and telegraphs.

## 3. Shapes and silhouettes
- Heroes: blocky, oversized hands, boots and weapons, plus a big helmet or visor read.
- Bugs: segmented, spiky, glossy. Each type has one unique silhouette feature you can read from 30m away.
- Weak points: one glowing spot per bug, in a color that contrasts with the bug's body.

## 4. Technical budgets (see ARCHITECTURE.md section 3)
| Asset | Triangles | Rig |
|---|---|---|
| Swarmer | ~300 | Vertex animation, no skeleton |
| Spitter | ~800 | Light skeleton |
| Brute | ~1,500 | Skeleton |
| Boss | up to 8,000 | Skeleton |
| Hero (3rd-person/lobby) | ~4,000 | Skeleton |
| First-person arms + gun | ~3,000 | Skeleton |
- One small palette texture per bug type; each type is drawn in one instanced batch.
- No real-time shadows on enemies; use a blob shadow decal.

## 5. Roster for the slice
- **Hero:** TBD after the camera decision.
- **Swarmer:** small skittering tick that moves in packs, with a glowing abdomen.
- **Spitter:** swollen glowing acid sac that brightens before it fires (attack telegraph).
- **Brute:** armored beetle. Its shell cracks and glows when vulnerable.
- **Brood mother (wave 5):** bloated egg-sac back; swarmers burst out of glowing pods.
- **Hive boss (wave 10):** TBD.

## 6. VFX language
- Player shots: cool-colored, crisp tracer cores.
- Bug attacks: warm, blobby, slow enough to dodge, always telegraphed 0.5s ahead.
- Hits: a short white flash plus a chunky colored splat. No gore realism.
- Pickups: rarity-colored glow with a vertical light beam.

## 7. UI
- Chunky rounded panels with bold outlines and a slight retro-sci-fi bevel.
- Rarity colors as above. Damage numbers stay small and fade fast.

## 8. Crate reveal
`content/crate_reveal.toml` is the source of truth for the storyboard. Shared beats: drop 0-2s, crack 2-3.5s, burst 3.5s. Legendary freezes for 1s, and its burst is at 4.5s. Rise ends at the card. Card time is per tier; these figures are a reference only: common 4.2s, rare 5.0s, epic 5.3s, legendary 6.5s.
- The result is decided before the drop. A fake-out flickers at most one tier up and always settles on the real tier.
- Skip works at any point. Ten-pulls end on a summary grid. In a ten-pull only the best card gets its full tier time; the others flip fast.

## 9. Telegraph honesty
- A warning glow (pods, veins, spitter sac) always means the attack or spawn really happens. Spawn telegraphs reserve room under the enemy cap (`reserve_on_telegraph = true` in `bosses.toml`).
- Fizzle VFX (the pod dims and puffs out a little green smoke) is used only when the boss dies or changes phase during the telegraph.
- A pod shot during its glow plays the normal kill splat, not the fizzle.

## 10. Bosses
- Brood mother (wave 5): `concepts/brood_mother_v1.jpg`. Pods glow 1.5s before a hatch, and the magenta gland under the head is the weak point.
- Hive tyrant (wave 10): `concepts/hive_tyrant_v1.jpg`. Phase 1 Armored (target the tail cannon), Phase 2 Cracked (chest core exposed, veins pulse before adds), Phase 3 Frenzy (armor shed, full core glow). Each phase change plays a shard burst and a roar during its 2.5s invulnerable window.

## 11. HUD layout (see concepts/hud_mockup_v1.jpg)
- Top center: wave counter + amber enemy-remaining bar. During bosses this swaps to a wide boss bar (amber fill, magenta when the weak point is exposed) with phase notches; the hive tyrant's notches are at 66% and 33%.
- Breather: the wave counter becomes a teal countdown; "+X" currency pops in gold above it at round end.
- Left: squad list of helmet icons in each player's accent color. Downed = icon turns grey with a red radial bleedout timer. An off-screen downed teammate gets a pulsing red-orange chevron at the screen edge plus distance.
- Bottom left: health bar (teal), jetpack fuel arc (orange), ability ring (white fill, flashes teal when ready).
- Bottom center, above the health panel: run upgrade icon row, max 12 shown, then "+N". Stack count bottom-right of each icon. Unique upgrades get a rarity-colored border; stackables have none.
- Bottom right: ammo. Center: small teal crosshair, with hit markers in white.
- No HUD element uses acid green or magenta except boss weak-point states, so bug colors stay meaningful.

## 12. Weak-point feedback VFX
- Spitter sac pop (cancel): magenta splash burst, sac deflates over 0.2s; the charge glow cuts on the same frame as `spitter_sac_pop`.
- Brute shell open: cracks flare bright magenta with a few sparks for the 2s window, then dim as they close.
- Weak-point hit: a larger white flash plus a magenta ring, and a slightly larger damage number. Normal hits keep the small white flash.

## 13. Code-built models
- Use `content/bugs_visual.toml` for proportions and anchor points only. Models are faceted low-poly with flat shading, overlapping segmented shell plates, chunky tapered jointed legs, and glow only on weak points.
- Build each mesh once per type and share it across that type, with a simpler version for distant bugs. Stay inside the triangle budgets in section 4.
- Review: the PR includes 3/4 front screenshots of each bug for comparison against the concepts.

## 14. Weak points without bloom
- Every glowing part also uses an unlit material in its palette color, so weak points stay readable with bloom off or on the WebGL2 fallback.
- `bugshots` fails if any glowing part shows no pixels in its palette color.
- Cosmetics may not use bug acid green or magenta as their main color.
- Brood mother: 6 large glowing live pods, plus 6 small dull buds that grow into pods on enrage.
