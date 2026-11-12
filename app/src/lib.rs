//! Desktop application logic that is testable without a window.
//!
//! Bevy-specific code stays in the binary (`main.rs`); pure logic such as the stepping
//! driver, the session that owns the simulation, the camera math and the drawing helpers
//! lives here so its unit tests need no renderer, input system or wall clock.

pub mod camera;
pub mod driver;
pub mod render;
pub mod session;

#[cfg(test)]
mod t_det_4;
