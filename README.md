# Project Gemini

Single-player milestone of a first-person co-op wave-defense shooter. The simulation is a deterministic Rust crate with no renderer and no networking. The browser client is Bevy 0.20, compiled twice: once for WebGPU and once for WebGL2. The page picks WebGPU when `navigator.gpu` exists, and `?api=webgl2` forces the fallback.

## Run the native client

From the repo root (assets load from `./assets`):

```bash
cargo run -p gemini-client
```

Linux needs the usual windowing libraries: `pkg-config`, `libwayland-dev`, `libxkbcommon-dev`, `libvulkan-dev`, `libasound2-dev`, `libudev-dev`, `libx11-dev`, `libxi-dev`, `libxcursor-dev`, `libxrandr-dev`.

The menu is keys `1`–`4` (Normal, Hard, Heroic, Legendary) or the on-screen buttons. In a match:

| Input | Action |
| --- | --- |
| WASD | Move |
| Mouse | Look (click to lock, Esc to release) |
| Space | Jump |
| Shift | Sprint |
| Ctrl | Crouch |
| F | Jetpack: tap to dash, hold to hover |
| Left mouse | Fire |
| R | Reload |
| Q | Ability |
| E | Interact. While downed solo, hold to self-revive |
| V | Toggle view bob |
| `[` `]` | Field of view |
| 1 2 3 | Pick a run upgrade between waves |
| Enter | Return to the menu after victory or defeat |
| F10 | Write front three-quarter stills to `art/bugs/` (native only) |

## Run in a browser

```bash
bash scripts/build-web.sh
python3 -m http.server 8080 -d web
```

Open `http://127.0.0.1:8080/` for WebGPU, or `http://127.0.0.1:8080/?api=webgl2` for the fallback. The script writes `web/pkg-webgl2`, `web/pkg-webgpu`, and copies `assets/audio` to `web/assets`. Audio starts after a click. The toolchain is Rust 1.97.1 (`rust-toolchain.toml`).

## Headless sim

```bash
cargo run -p gemini-server
cargo test --workspace
```

`gemini-server` steps the same sim for 180 ticks and prints wave state. It does not open a socket.

Bug stills without the client:

```bash
cargo run -p gemini-meshes --bin bugshots -- --out art/bugs
```

## Layout

- `crates/sim` — content tables, movement, combat, waves, upgrades, spatial grid, music clocks. Serializable with bincode.
- `crates/meshes` — one shared flat-shaded mesh per bug part, plus a coarser LOD mesh per type. Gameplay never reads these meshes.
- `crates/client` — first-person camera, instanced bugs, HUD, stem mixer, sound hooks.
- `crates/server` — headless smoke runner.
- `crates/proto` — versioned envelopes for a later host.
- `content/` — designer TOML. The sim embeds these files with `include_str!`.
- `assets/audio/` — silent OGG Vorbis placeholders at 48 kHz. Real files drop in under the same names.
- `art/bugs/` — front three-quarter renders of the five procedural bugs.

## What is stubbed

Silent audio. Procedural low-poly bugs and arms stand in for final art. No multiplayer, accounts, currency persistence, or crate-opening UI. Endless mode, banners, and duplicate ranks load from content and are not entered by the 10-wave match. The server does not listen on a port. Crate-reveal timings stay out of code until `content/crate_reveal.toml` exists.
