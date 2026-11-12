//! Desktop application logic that is testable without a window.
//!
//! Bevy-specific code stays in the binary (`main.rs`). Pure logic (the stepping driver,
//! the session that owns the simulation, camera math, drawing helpers, presets and frame
//! statistics) and the egui panels live here, so their unit tests need no renderer, input
//! system or wall clock.

pub mod camera;
pub mod driver;
pub mod perf;
pub mod presets;
pub mod render;
pub mod session;
pub mod ui;

#[cfg(test)]
mod t_det_4;
