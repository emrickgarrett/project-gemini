# Project Gemini: architecture v0.1

Web-only, first-person, co-op wave defense for up to 8 players. Written in Rust.
Owners: El Gorko (architecture and review), Gork (build), Taylor Gork (audio).

## 1. Decisions locked by Garrett
- Upgrades found during a run reset after every match. Progression that carries over comes only from crates.
- Duplicate characters give stat upgrades plus cosmetic and ability variants. The game is PvE co-op.
- There are several difficulty tiers, and higher tiers give better rewards.
- There are boss waves, a mode with an ending, and an endless mode that keeps getting harder.
- Players can join a match already in progress. The same path handles reconnects.
- The game is free. Paid crates are only a later footnote, so we build to keep that option open (section 8).

## 2. Repo layout (Cargo workspace)
```
crates/
  sim/       deterministic game simulation: ECS components, systems, waves, damage. No rendering, no I/O.
  proto/     network messages (serde + bincode), versioned
  client/    Bevy app compiled to wasm32: rendering, input, prediction, audio, UI
  server/    headless match server (Bevy MinimalPlugins + sim), authoritative
  backend/   accounts, currency, inventory and crate API (axum + sqlx + Postgres)
  content/   data files for heroes, weapons, enemies, waves, upgrades and loot tables (TOML)
web/         static host page, wasm loader, asset manifest
```
The rule is that `sim` has no rendering and no networking code. The client and the server run the same `sim`.

## 3. Client
- Bevy compiled to wasm32-unknown-unknown, rendering through WebGPU with a WebGL2 fallback.
- Art budget for the Jet Force Gemini look: low-poly models with vertex colors or small palette textures, emissive projectiles plus bloom, and no real-time shadows on enemies.
- Enemies are drawn with GPU instancing, one draw call per enemy type and LOD.
- Performance targets: 60 fps on an integrated-GPU laptop with 8 players and 300 live enemies, and a first wasm download under 15 MB gzipped. Assets stream in per map.
- The camera is first-person, and pointer lock is used for the controls. Cosmetics are shown off in the lobby, on the crate reveal, and in third-person emotes or a kill-cam.

## 4. Networking
- **v1 is player-hosted, as a listen server.** The host's browser tab runs the authoritative `sim` alongside its own client. The other players connect peer-to-peer over WebRTC data channels, using an unreliable channel for snapshots and a reliable one for events.
- Browsers can't accept inbound connections, so v1 still needs two small pieces of infrastructure. The first is a signaling and lobby service, which is an invite code plus SDP exchange added to `backend/`. The second is STUN plus a TURN relay for players behind strict NAT, which is about 10–20% of them, using a managed TURN service or coturn on a VPS.
- **Dedicated servers later** reuse the same `server` crate, because the netcode sits behind one transport trait. The candidates are a VPS to start, then Edgegap or Fly.io for servers placed near players.
- The server simulates at 30 Hz.
- Each client sends input every tick. The server sends delta snapshots with priority and interest management: enemies far away update less often.
- The client predicts its own movement and firing and reconciles with the server. Remote entities are interpolated about 100 ms behind.
- Hits are checked with lag compensation, by rewinding hitboxes up to 200 ms.
- **Joining mid-match:** a new player gets a full snapshot, then deltas. They spawn at the next safe point with a catch-up kit (section 6).
- **Reconnects:** a session token keeps the player's slot and run upgrades for a 120 s grace period.
- **Host leaving:** in v1 the match ends and everyone keeps the currency they've earned so far, since rewards are credited after each round. Migrating the host to another player is a stretch goal, made possible by the same snapshot that join-in-progress uses.

## 5. Backend (persistent progression)
- Postgres tables: accounts, wallets, inventory, characters (including duplicate count), crate_pulls (an audit log), and match_results.
- **Rewards in v1:** the host is a player's browser, so its results can't be fully trusted. The backend credits currency using a formula it controls (wave reached, difficulty, time played, players present) and rejects anything implausible, such as waves cleared faster than physically possible or rewards above the cap per hour. It also rate-limits per account. Crate rolls stay on the backend no matter what. Dedicated servers would later restore full authority.
- Crate rolls happen only on the backend: the loot table plus a server-side RNG seed, logged per pull. Pity counters are stored per banner.
- The client gets the pull result first and then plays the reveal animation. The animation is never what decides the outcome.

## 6. Game systems (all in `sim`)
- **Waves:** each wave definition lists a budget, enemy types and spawn groups. Boss waves are authored separately.
- **Difficulty:** a multiplier table covers enemy HP, damage, count and reward. Each tier shows a recommended power level in the lobby.
- **Endless mode:** once live enemies reach the cap, difficulty scales through HP and damage instead of count. Multipliers are stored as f64 or fixed-point with clamps, so they can't overflow by wave 200.
- **Leftover budget:** budget that waits while the 300-enemy cap is full carries into the next wave, up to one wave's budget, and anything over that is dropped.
- **Run upgrades:** pickups on the map plus a choose-one-of-three screen between rounds. They live only in match state.
- **Persistent power:** character level, duplicate rank and weapon parts are loaded into the match as stat modifiers when a player joins.
- **Catch-up kit:** a player who joins late gets about 70% of the average number of upgrade picks the team has, so they aren't dead weight.

## 7. Audio integration
- Each combat loop arrives as a set of stems at a fixed tempo and key, all the same bar length. They are played sample-synced on one clock.
- The music director turns layers on and off according to wave intensity. Boss tracks cut in on the next downbeat.
- Higher difficulties start with more layers already playing. In endless mode the music rotates between loop variants.
- Crate stings are single sound files, one for each rarity.
- Enemy sound effects are limited to 32 voices, prioritized by distance and loudness. Swarmers share one crowd loop whose volume follows how many are nearby.

## 8. Keeping monetization open (footnote)
- Every banner shows its drop rates and pity rules from day one.
- All pulls are logged on the server.
- Currencies are kept separate: the free currency is earned in play, and a premium currency is reserved but not built.
- Before anything is sold, we need a legal check on loot box rules by region (Belgium, the Netherlands, China and others require odds disclosure or restrict paid loot boxes).

## 9. Milestones and their gates
| # | Milestone | Ships when |
|---|-----------|-----------|
| M1 | Vertical slice: one map, one hero, three enemy types, 10 waves, single player in the browser | 60 fps target met, and Garrett says it's fun |
| M2 | Two-player co-op, then 8 players, with join-in-progress and reconnect | 8 bots plus 300 enemies with no desync for 30 minutes, and a reconnect test passes |
| M3 | Roguelike layer: map pickups and between-round choices | Upgrade pool of at least 20, and runs feel different from each other |
| M4 | Backend: accounts, currency, one banner, the reveal animation | Server-only rolls, audit log, odds shown |
| M5 | Content: maps, heroes, boss waves, endless mode, difficulty tiers | Reached by expanding the list one item at a time |

Each milestone ships only when its gate passes. "It compiles" is not a gate.

## 10. Open questions
Resolved by Garrett on 2026-10-10:
- The camera is first-person.
- Duplicate stat bonuses apply on every difficulty.
- v1 is player-hosted with invites, and dedicated hosting comes later.
- Players start on a guest account, and linking an account comes later.
- Garrett is on Suno's top plan, so commercial use is covered.
- Images are made with our own tools or Garrett's local game harness.

Still open: whether the guest account is tied to the browser, and how to warn players about losing progress until they link an account.
