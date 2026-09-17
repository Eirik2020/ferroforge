//! A hardware task that needs the STM32F4 HAL.
//!
//! Nothing here is abstracted. The resource has the concrete HAL type the
//! firmware will hold, and the body is the ordinary handler you would write by
//! hand: read the update flag, clear it, count. That is the point of G1's
//! "HAL-specific tasks reusable across projects using that HAL" - reuse across
//! projects, not across HALs, and not across peripherals.
//!
//! The interrupt is still named by the composition, not here.

#![no_std]

use stm32f4xx_hal::{
    ClearFlags, ReadFlags,
    pac::TIM3,
    timer::{CounterUs, Flag},
};

#[ferroforge::task(
    local = [timer: CounterUs<TIM3>, elapsed: u32],
    spawn = [elapsed(count: u32)],
)]
pub fn on_timer(cx: on_timer::Context) {
    if !cx.local.timer.flags().contains(Flag::Update) {
        // Something else sharing this interrupt raised it.
        return;
    }
    // Not optional: an uncleared flag re-enters the handler on return.
    cx.local.timer.clear_flags(Flag::Update);

    *cx.local.elapsed = cx.local.elapsed.wrapping_add(1);
    let _: Result<(), u32> = cx.spawn.elapsed(*cx.local.elapsed);
}
