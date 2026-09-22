//! MSP v1 and MSP DisplayPort: the protocol a flight controller uses to draw
//! an OSD in a pilot's goggles.
//!
//! Ordinary Rust with no dependencies at all - no HAL, no RTIC, no FerroForge.
//! That is the point of it being a separate crate from the tasks that move the
//! bytes: framing, formatting and the reply table are the parts a host test can
//! reach, and the parts that were worth testing before wiring anything up.
//!
//! - [`codec`] is the wire format.
//! - [`displayport`] builds the subcommands that draw.
//! - [`poll`] answers what the transmitter asks before it will draw at all.
//! - [`queue`] holds outgoing frames whole or not at all.
//! - [`screen`] is the text canvas, kept separate from both.

#![no_std]
#![forbid(unsafe_code)]

pub mod codec;
pub mod displayport;
pub mod poll;
pub mod queue;
pub mod screen;

pub use codec::{Direction, Error, MAX_FRAME, MAX_PAYLOAD, Packet, Parser, encode};
pub use poll::Telemetry;
pub use queue::Frames;
pub use screen::{COLS, Line, ROWS, Screen};
