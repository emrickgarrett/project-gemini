use std::net::TcpListener;
use std::thread;

use gemini_server::{client_exchange, host_exchange};
use gemini_sim::sim::{Phase, Sim};
use gemini_sim::Content;

#[test]
fn listen_server_hosts_a_second_peer() {
    let content = Content::embedded();
    let mut sim = Sim::new(&content, "normal", 3);
    sim.phase = Phase::Breather { left: 30.0 };
    sim.enemies.clear();
    sim.pending_vents.clear();
    sim.director.pool = 0.0;
    sim.director.parked = true;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let host = thread::spawn(move || host_exchange(listener, &mut sim, &content, 20));
    let guest = thread::spawn(move || client_exchange(addr, 20, None));
    let guest_sim = guest.join().expect("guest");
    let host_sim = host.join().expect("host");
    assert_eq!(host_sim.players.len(), 2, "joiner should spawn during the breather");
    assert_eq!(guest_sim.players.len(), 2);
    assert!(guest_sim.players[1].connected);
    assert!(
        guest_sim.players[1].pos.z < -0.5,
        "guest input should move the joined seat, z={}",
        guest_sim.players[1].pos.z
    );
    assert_eq!(host_sim.to_bytes().unwrap(), guest_sim.to_bytes().unwrap());
}
