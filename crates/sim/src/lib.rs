//! Deterministic match simulation shared by the browser client and any future host.
//! No renderer, no sockets, no asset I/O.

pub mod audio_mix;
pub mod bugs;
pub mod content;
pub mod host_away;
pub mod reveal;
pub mod math;
pub mod music;
pub mod ogg;
pub mod rng;
pub mod sim;
pub mod spatial;
pub mod upgrade;

pub use content::Content;
pub use math::Vec3;
pub use sim::{Phase, PlayerInput, Sim, SIM_DT, SIM_HZ};
