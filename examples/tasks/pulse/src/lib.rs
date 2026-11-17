//! A group built from another crate's group: a light from `tasks/blinky`, and
//! a hardware task here that counts pulses and announces each count through
//! the light's own announcer.
//!
//! Nothing here names a HAL or a chip. The hardware member is synchronous, so
//! a firmware must bind it to an interrupt, and which one is the firmware's
//! choice. It counts the interrupt and hands the count on; acknowledging the
//! peripheral that raised it is the firmware's business, as it is in any
//! handler that does not own its peripheral. A HAL-specific group, which does
//! own one, lives in a HAL helper crate such as `ferroforge-stm32`.

#![no_std]

/// One interrupt, counted, and the count handed on.
#[ferroforge::task(local = [pulses: u32], spawn = [pulse(count: u32)])]
pub fn on_pulse(cx: on_pulse::Context) {
    *cx.local.pulses = cx.local.pulses.wrapping_add(1);
    let _: Result<(), u32> = cx.spawn.pulse(*cx.local.pulses);
}

/// A light whose announcer also reports pulses: `blinky`'s `light` group,
/// from another crate, with `on_pulse` wired to its `announce` inside this
/// group. A firmware selecting it binds the union of their resources once.
#[ferroforge::group(spawn = [pulse = announce])]
pub mod pulse_light {
    pub use super::on_pulse;
    pub use ferroforge_task_blinky::light::*;
}
