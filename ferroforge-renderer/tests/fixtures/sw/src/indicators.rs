use embedded_hal::digital::StatefulOutputPin;
use ferroforge::{mock::systick::Mono, task};
use fugit::ExtU32 as _;

pub mod details;

struct Sample(u32);

impl Sample {
    fn increment(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
}

fn count(value: &mut u32) {
    *value = value.wrapping_add(1);
}

// This retained support item keeps `heapless` in the conservative package-level
// dependency set even when no selected task calls it.
fn scratch() -> heapless::Vec<u8, 4> {
    heapless::Vec::new()
}

#[task(
    bounds = [led: StatefulOutputPin],
    local = [led, count: u32],
    shared = [enabled: bool],
    config = [period_ms: u32],
    spawn = [report(value: u32)],
    monotonic = Mono,
)]
pub async fn blink(mut cx: blink::Context) -> ! {
    loop {
        if cx.shared.enabled.lock(|flag| *flag) {
            let _ = StatefulOutputPin::toggle(&mut *cx.local.led);
            count(cx.local.count);
            let _: Result<(), u32> = cx.spawn.report(*cx.local.count);
        }
        Mono::delay(CONFIG.PERIOD_MS.millis()).await;
    }
}

#[task]
pub async fn report(cx: report::Context, value: u32) {
    defmt::info!("count={=u32}", value);
}

#[task(spawn = [wake(), pair(left: u16, enabled: bool)])]
pub async fn spawn_shapes(cx: spawn_shapes::Context) {
    let _: Result<(), ()> = cx.spawn.wake();
    let _: Result<(), (u16, bool)> = cx.spawn.pair(1, true);
}
