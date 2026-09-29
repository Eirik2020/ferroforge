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

/// One light: the blink task alone. A group of one is still worth having,
/// because a group can include it more than once.
#[ferroforge::group]
pub mod light {
    use super::*;

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
}

/// Where blink counts go.
#[ferroforge::group]
pub mod reporter {
    #[ferroforge::task]
    pub async fn report(_cx: report::Context, value: u32) {
        defmt::info!("blink count={=u32}", value);
    }
}

/// Two lights reporting to one place: the same group twice, each copy under its
/// own name, so every name it has is prefixed - `heartbeat_led`,
/// `beacon_period_ms`, `heartbeat_blink`. Both copies' reports are wired to the
/// one reporter, which is merged in unprefixed.
#[ferroforge::group(spawn = [heartbeat_report = report, beacon_report = report])]
pub mod lights {
    pub use super::light as heartbeat;
    pub use super::light as beacon;
    pub use super::reporter::*;
}

// Each task is still selectable on its own, from where it always was.
pub use light::blink;
pub use reporter::report;

/// Synchronous, so the contract reads it as a hardware task. The definition
/// never names an interrupt: composition binds one, so the same handler can
/// serve different interrupts in different firmware.
#[ferroforge::task(local = [ticks: u32])]
pub fn on_tick(cx: on_tick::Context) {
    increment(cx.local.ticks);
}
