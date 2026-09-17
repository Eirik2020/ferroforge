//! The STM32H7 counterpart to `tasks/stm32f4-timer`, and the evidence that the
//! HAL-specific task pattern is not specific to one HAL.
//!
//! The work is the same - read and clear a timer's interrupt flag, which no
//! portable trait models - but the API is not: this HAL spells it `clear_irq`
//! on its own `Timer`, where the F4 HAL uses `ClearFlags` on an `FTimer`. That
//! difference is exactly why a task touching it is HAL-specific.

#![no_std]

use stm32h7xx_hal::{pac::TIM2, timer::Timer};

#[ferroforge::task(
    local = [timer: Timer<TIM2>, elapsed: u32],
    spawn = [elapsed(count: u32)],
)]
pub fn on_timer(cx: on_timer::Context) {
    if cx.local.timer.is_irq_clear() {
        // Something else sharing this interrupt raised it.
        return;
    }
    // Not optional: an uncleared flag re-enters the handler on return.
    cx.local.timer.clear_irq();

    *cx.local.elapsed = cx.local.elapsed.wrapping_add(1);
    let _: Result<(), u32> = cx.spawn.elapsed(*cx.local.elapsed);
}
