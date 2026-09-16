//! The same `blink`/`report` behaviour as `tasks/blinky`, authored as ordinary
//! Rust with real trait bounds instead of a FerroForge mock context.
//!
//! Nothing here is transplanted. The firmware depends on this crate normally
//! and its RTIC handlers call these functions, so the bodies are compiled once,
//! in place, by the same compiler that checks them standalone.

#![no_std]

use embedded_hal::digital::StatefulOutputPin;
use fugit::ExtU32 as _;
use rtic::Mutex;
use rtic_monotonics::Monotonic;

fn increment(value: &mut u32) {
    *value = value.wrapping_add(1);
}

/// What `blink` requires, as a real struct in this crate. The firmware builds
/// one from its own RTIC context; the field names stay the reusable ones, so
/// the body below is unchanged from the authored version.
pub struct BlinkContext<'a, Led, Enabled> {
    pub led: &'a mut Led,
    pub count: &'a mut u32,
    pub enabled: Enabled,
}

/// `Mono` is a type parameter rather than a concrete monotonic, so this crate
/// never names the firmware's clock. `Spawn` stands in for the outgoing
/// `report` alias.
/// `PERIOD_MS` is a const generic, so composition's configuration value is a
/// compile-time constant inside the body - usable in array lengths and other
/// const positions, not only as a runtime read.
pub async fn blink<Led, Enabled, Mono, Spawn, const PERIOD_MS: u32, const HISTORY: usize>(
    mut cx: BlinkContext<'_, Led, Enabled>,
    report: Spawn,
) -> !
where
    Led: StatefulOutputPin,
    Enabled: Mutex<T = bool>,
    Mono: Monotonic<Duration = fugit::Duration<u32, 1, 1000>>,
    Spawn: Fn(u32) -> Result<(), u32>,
{
    // A const parameter used as a standalone argument: a real const position,
    // not a runtime value. Arithmetic on it would need `generic_const_exprs`.
    let mut recent = [0u32; HISTORY];
    let mut next = 0usize;
    loop {
        if cx.enabled.lock(|enabled| *enabled) {
            let _ = StatefulOutputPin::toggle(cx.led);
            increment(cx.count);
            recent[next % recent.len()] = *cx.count;
            next = next.wrapping_add(1);
            let _: Result<(), u32> = report(*cx.count);
        }
        Mono::delay(PERIOD_MS.millis()).await;
    }
}

pub async fn report(value: u32) {
    defmt::info!("blink count={=u32}", value);
}
