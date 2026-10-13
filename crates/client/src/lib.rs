//! Shared by the game binary and the pull-screen stills. No Bevy types.

#[cfg(all(feature = "dev-host", not(debug_assertions)))]
compile_error!("dev-host is a debug stand-in and cannot be built in release");

pub mod reveal_paint;
