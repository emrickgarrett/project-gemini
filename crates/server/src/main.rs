//! Headless smoke run. M1 does not open a socket; M2 hosts this behind a transport.

use gemini_sim::sim::{Phase, PlayerInput, Sim};
use gemini_sim::Content;

fn main() {
    let content = Content::embedded();
    let mut sim = Sim::new(&content, "normal", 11);
    let input = PlayerInput {
        move_z: 1.0,
        fire: true,
        yaw: 0.4,
        ..PlayerInput::default()
    };
    for _ in 0..180 {
        sim.step(&content, input);
    }
    let phase = match sim.phase {
        Phase::Combat => "combat",
        Phase::Upgrade { .. } => "upgrade",
        Phase::Breather { .. } => "breather",
        Phase::Victory => "victory",
        Phase::Defeat => "defeat",
    };
    println!(
        "gemini-server smoke tick={} wave={} left={} phase={phase}",
        sim.tick,
        sim.director.wave,
        sim.enemies_left()
    );
}
