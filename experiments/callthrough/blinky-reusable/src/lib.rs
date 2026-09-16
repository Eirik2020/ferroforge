//! `tasks/blinky` verbatim, with `#[ferroforge::task]` swapped for
//! `#[ferroforge::reusable]`. The declarations and both bodies are unchanged:
//! the only edit is the attribute name.
//!
//! If this checks standalone for ARM, the call-through expansion preserves the
//! authored model - `cx.local`, `cx.shared.lock`, `cx.spawn`, `CONFIG.FIELD`
//! and `Mono::delay` all still mean what they meant.

#![no_std]

use embedded_hal::digital::StatefulOutputPin;
use fugit::ExtU32 as _;

fn increment(value: &mut u32) {
    *value = value.wrapping_add(1);
}

#[ferroforge::reusable(
    bounds = [led: StatefulOutputPin],
    local = [led, count: u32],
    shared = [enabled: bool],
    config = [period_ms: u32],
    spawn = [report(value: u32)],
    monotonic = Mono,
)]
pub async fn blink(mut cx: blink::Context) -> ! {
    loop {
        if cx.shared.enabled.lock(|enabled| *enabled) {
            let _ = StatefulOutputPin::toggle(&mut *cx.local.led);
            increment(cx.local.count);
            let _: Result<(), u32> = cx.spawn.report(*cx.local.count);
        }
        Mono::delay(CONFIG.PERIOD_MS.millis()).await;
    }
}

#[ferroforge::reusable]
pub async fn report(_cx: report::Context, value: u32) {
    defmt::info!("blink count={=u32}", value);
}
