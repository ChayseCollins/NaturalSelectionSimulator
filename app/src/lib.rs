//! Desktop application logic that is testable without a window.
//!
//! Bevy-specific code stays in the binary; pure logic such as the stepping driver lives
//! here so its unit tests need no renderer, input system or wall clock.

pub mod driver;
