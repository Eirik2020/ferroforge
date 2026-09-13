#![no_std]

use embedded_hal::digital::StatefulOutputPin;
use ferroforge::mock::systick::Mono;
use fugit::ExtU32 as _;

fn increment(value: &mut u32) {
    *value = value.wrapping_add(1);
}

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
        Mono::delay(CONFIG.PERIOD_MS.millis()).await;
    }
}

#[ferroforge::task]
pub async fn report(_cx: report::Context, value: u32) {
    defmt::info!("blink count={=u32}", value);
}
