# Project Gemini SFX

All sounds except brood_pod_hatch.ogg and tyrant_roar.ogg are procedurally synthesized in Python (numpy/scipy, /workspace/suno/sfx.py), no samples. Those two are trimmed Suno Sounds clips (pod hatch take 2, 1.2 s, burst peaks ~0.95 s, see bosses.toml hatch_sfx_peak_offset; tyrant roar take 2, 2.9 s) generated under Garrett's paid Suno plan, so commercial use is permitted. First-pass placeholders; nobody has listened to them yet. OGG Vorbis q6, 48 kHz mono, peaks normalized to -2 dBFS. Loops (L) wrap seamlessly via tail-to-head crossfade. Playback levels, pitch randomization and voice limits live in content/audio_mix.toml.

| File | Duration (s) | Loop | Trigger |
|---|---|---|---|
| blaster_fire.ogg | 0.260 |  | Player fires blaster (pick randomly among 01/02/03 variants) |
| blaster_fire_02.ogg | 0.260 |  | Blaster fire variation |
| blaster_fire_03.ogg | 0.260 |  | Blaster fire variation |
| blaster_reload.ogg | 0.900 |  | Blaster reload / energy recharge |
| jetpack_boost.ogg | 0.550 |  | Jetpack dash burst |
| jetpack_hover_loop.ogg | 2.000 | L | Jetpack hover (loop while hovering) |
| jetpack_empty.ogg | 0.950 |  | Jetpack out of fuel |
| fuel_regen_paused.ogg | 0.200 |  | Plays once when jetpack regen is paused on a perch. |
| pickup.ogg | 0.670 |  | Pickup collected |
| upgrade_pick.ogg | 0.780 |  | Between-wave upgrade confirmed |
| player_hurt.ogg | 0.350 |  | Player takes damage |
| player_downed.ogg | 1.600 |  | Player downed |
| revive_complete.ogg | 1.348 |  | Revive finished |
| swarmer_click_01.ogg | 0.055 |  | Swarmer movement/attack click (random) |
| swarmer_click_02.ogg | 0.056 |  | Swarmer movement/attack click (random) |
| swarmer_click_03.ogg | 0.056 |  | Swarmer movement/attack click (random) |
| swarmer_click_04.ogg | 0.057 |  | Swarmer movement/attack click (random) |
| swarmer_crowd_loop.ogg | 4.000 | L | Swarm crowd bed, one voice scaled by swarm size |
| swarmer_death.ogg | 0.287 |  | Swarmer dies |
| spitter_charge.ogg | 1.000 |  | Spitter fire warning: exactly 1.0 s, play at telegraph start |
| spitter_spit.ogg | 0.350 |  | Spitter launches acid |
| acid_impact.ogg | 0.600 |  | Acid glob hits ground/player (hiss) |
| spitter_death.ogg | 0.800 |  | Spitter dies (sac bursts) |
| brute_step_01.ogg | 0.500 |  | Brute footstep |
| brute_step_02.ogg | 0.500 |  | Brute footstep |
| brute_step_03.ogg | 0.500 |  | Brute footstep |
| brute_grunt.ogg | 0.700 |  | Brute attack grunt |
| brute_shell_crack.ogg | 0.796 |  | Brute shell cracks (enters vulnerable state) |
| brute_death.ogg | 1.600 |  | Brute dies (collapse thud at end) |
| brood_drone_loop.ogg | 4.000 | L | Brood mother ambient drone (loop during fight) |
| brood_pod_charge.ogg | 1.500 |  | Brood pod hatch telegraph: exactly 1.5 s |
| brood_pod_hatch.ogg | 1.200 |  | Pod hatches (spawns swarmers). Source: Suno Sounds, https://suno.com/song/073e65b1-5202-49b7-b9b3-c74f2df729d6 (take 2, trimmed 0.000-1.200s) |
| brood_pod_fizzle.ogg | 0.990 |  | Pod destroyed in time (deflates, no spawn) |
| brood_roar.ogg | 2.000 |  | Brood mother roar |
| brood_death.ogg | 2.700 |  | Brood mother dies |
| tyrant_roar.ogg | 2.900 |  | Hive tyrant roar on phase change. Source: Suno Sounds, https://suno.com/song/8acd36af-d8a5-4dc5-90a5-3416a092e99a (take 2, trimmed 0.000-2.900s) |
| tyrant_tail_charge.ogg | 1.200 |  | Tyrant tail-attack wind-up |
| tyrant_scythe_swipe.ogg | 0.500 |  | Tyrant scythe swipe whoosh |
| tyrant_death.ogg | 4.100 |  | Hive tyrant dies |
| wave_start.ogg | 0.660 |  | Wave begins |
| wave_clear.ogg | 1.200 |  | Wave cleared |
| ui_click.ogg | 0.050 |  | UI button click |
| currency_gain.ogg | 0.450 |  | Currency earned |
| spitter_sac_pop.ogg | 0.298 |  | Spitter sac popped mid-charge: spit cancelled (stop spitter_charge immediately) |
| brute_shell_open.ogg | 0.500 |  | Brute shell cracks open: start of 2 s vulnerable window (shoot now) |
| weakpoint_hit.ogg | 0.150 |  | Weak-point / critical hit (2.5x brute cracks, spitter sac, etc.) |
| player_join.ogg | 0.500 |  | Co-op player joined (rising G5-D6 chime) |
| player_leave.ogg | 0.500 |  | Co-op player left (falling D6-G5 chime) |
| invite_received.ogg | 0.560 |  | Co-op invite received (attention ping) |
| reconnect.ogg | 0.500 |  | Reconnected / state re-synced |
| currency_tick.ogg | 0.050 |  | Currency counter tick, one per increment |
| crate_open.ogg | 0.787 |  | Crate latch + lid whoosh |
| crate_tease.ogg | 1.500 |  | Rarity tease build; start at crate_reveal crack (2.0 s) so it peaks at burst (3.5 s) |
| crate_tease_legendary.ogg | 2.500 |  | Legendary tease; start at crack (2.0 s): same rise to 3.5 s, then tense shimmer through the freeze (3.5-4.5 s), dropping to a near-silent suspended tone in the last ~0.3 s so the 4.5 s burst hits from quiet |
| crate_fakeout.ogg | 0.370 |  | Fake-out flicker (rare/epic/legendary only, where crate_reveal fakeout_allowed = true): bright rising shimmer like a next-tier reveal start, hard-cut at ~0.28 s with a tiny 'denied' down-blip; play at flicker start. crate_reveal.toml gives no flicker time or length |
| dupe_upgrade.ogg | 1.000 |  | Duplicate converted to stat upgrade (power-up chord) |
| vex_fire.ogg | 0.150 |  | Vex pulse_rifle shot (9 rps auto; punchy mid pulse) |
| vex_reload.ogg | 1.226 |  | Vex pulse_rifle reload (1.6 s: mag out, charge whine, mag in, bolt) |
| vex_ability.ogg | 0.900 |  | Vex overdrive activate (6 s: +35% fire rate, infinite mag) rev-up |
| vex_ability_end.ogg | 0.600 |  | Vex overdrive ends (spin-down) |
| nyra_fire.ogg | 0.200 |  | Nyra arc_caster bolt (6 rps; electric crackle that chains) |
| nyra_reload.ogg | 1.279 |  | Nyra arc_caster reload (1.8 s: cell out, recharge whine, cell in) |
| nyra_ability.ogg | 1.000 |  | Nyra mend_field deployed (6 m heal field, 8 s) bloom |
| nyra_ability_loop.ogg | 2.000 | L | Nyra mend_field active hum (loop while field lasts, 8 s) |
| nyra_ability_end.ogg | 0.660 |  | Nyra mend_field expires (falling chime) |
| brakk_fire.ogg | 0.700 |  | Brakk scatter_cannon blast (1.4 rps, 10 pellets; heavy low boom) |
| brakk_reload.ogg | 1.811 |  | Brakk scatter_cannon reload (2.4 s: shells loaded, heavy pump) |
| brakk_ability.ogg | 0.700 |  | Brakk bulwark raise (4 s frontal shield) energy clamp |
| brakk_ability_end.ogg | 1.100 |  | Brakk bulwark ends in shield slam (150 dmg + knockback) |
| kip_fire.ogg | 1.100 |  | Kip rail_needle shot (1.2 rps, pierces 3; sharp crack + long tail) |
| kip_reload.ogg | 1.757 |  | Kip rail_needle reload (2.0 s: cell swap, capacitor charge ping) |
| kip_ability.ogg | 1.280 |  | Kip mark ping (15 m, 6 s: weak points through walls) sonar |
| kip_ability_end.ogg | 0.480 |  | Kip mark expires (soft descending blip) |
| part_equip.ogg | 0.422 |  | Weapon part (barrel/core/mod) slotted in: mechanical click-in + small power hum |
| part_unequip.ogg | 0.222 |  | Weapon part removed: hum powers down + release click |
| shard_gain.ogg | 0.483 |  | Duplicate part/cosmetic converted to shards (crystalline tinkle) |
| reroll_spin.ogg | 1.000 |  | Secondary-stat reroll spinning (shards spent): fast rising ticks, ~1 s; follow with reroll_result |
| reroll_result.ogg | 0.540 |  | Reroll lands on new secondary stat (settle chime) |
| legendary_perk_reveal.ogg | 1.299 |  | Legendary part unique perk revealed (bigger/brighter than dupe_upgrade) |
| cosmetic_equip.ogg | 0.438 |  | Cosmetic equipped (soft swish + light chime) |
| vent_warn.ogg | 1.500 |  | Arena vent telegraph: exactly 1.5 s, play when a vent starts its telegraph |
| vent_burst.ogg | 0.500 |  | Arena vent opens and the bug comes out |
| perch_climb_warn.ogg | 1.500 |  | Swarmer starts climbing a resin streak toward a perch (anti-camp) |
