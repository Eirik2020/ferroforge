//! Portable task definitions: nothing here names a HAL or a chip, only
//! `embedded-hal` traits and the bounds each task declares. Contrast
//! `tasks/stm32f4-timer`, which needs its HAL and says so.
//!
//! That this checks standalone for ARM is the evidence that the call-through
//! expansion preserves the authored model - `cx.local`, `cx.shared.lock`,
//! `cx.spawn`, `CONFIG::FIELD` and `Mono::delay` all still mean what they mean
//! in ordinary RTIC.

#![no_std]

use embedded_hal::digital::StatefulOutputPin;
use fugit::ExtU64 as _;

fn increment(value: &mut u32) {
    *value = value.wrapping_add(1);
}

/// One light. A task is also a group of one, so a group can include it by
/// name, as many times as it likes.
#[ferroforge::task(
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
        Mono::delay(u64::from(CONFIG::PERIOD_MS).millis()).await;
    }
}

/// Where blink counts go.
#[ferroforge::task]
pub async fn report(_cx: report::Context, value: u32) {
    defmt::info!("blink count={=u32}", value);
}

/// Two lights reporting to one place: the same task twice, each copy under its
/// own name, so every name it has is prefixed - `heartbeat_led`,
/// `beacon_period_ms`, `heartbeat_blink`. Both copies' reports are wired to the
/// one reporter, which is merged in unprefixed.
#[ferroforge::group(spawn = [heartbeat_report = report, beacon_report = report])]
pub mod lights {
    pub use super::blink as heartbeat;
    pub use super::blink as beacon;
    pub use super::report;
}

/// Synchronous, so the contract reads it as a hardware task. The definition
/// never names an interrupt: composition binds one, so the same handler can
/// serve different interrupts in different firmware.
#[ferroforge::task(local = [ticks: u32])]
pub fn on_tick(cx: on_tick::Context) {
    increment(cx.local.ticks);
}
