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

/// Where one light's counts go, under the name its firmware gives it, so two
/// lights in one log can be told apart line by line.
#[ferroforge::task(config = [label: &'static str])]
pub async fn announce(_cx: announce::Context, value: u32) {
    defmt::info!("{=str} count={=u32}", CONFIG::LABEL, value);
}

/// One light and its own log line: a group of two, with the light's `report`
/// wired to its announcer inside the group.
#[ferroforge::group(spawn = [report = announce])]
pub mod light {
    pub use super::announce;
    pub use super::blink;
}

/// Two lights, each announcing itself, and a reporter for whatever else the
/// firmware wants counted: the `light` group twice, each copy under its own
/// name so every name it has is prefixed - `heartbeat_led`, `beacon_label`,
/// `heartbeat_announce` - and each copy wired inside itself. The reporter is
/// merged in unprefixed.
#[ferroforge::group]
pub mod lights {
    pub use super::light as heartbeat;
    pub use super::light as beacon;
    pub use super::report;
}

/// Synchronous, so the contract reads it as a hardware task. The definition
/// never names an interrupt: composition binds one, so the same handler can
/// serve different interrupts in different firmware.
#[ferroforge::task(local = [ticks: u32])]
pub fn on_tick(cx: on_tick::Context) {
    increment(cx.local.ticks);
}
