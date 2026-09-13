use ferroforge::task;

#[task(
    config = [period_ms],
    local = [led],
    shared = [enable_blink],
    dependencies = [Fugit, Defmt],
)]
pub(crate) async fn blink(mut cx: blink::Context) -> ! {
    use crate::Mono;
    use fugit::ExtU64 as _;

    loop {
        let enabled = cx.shared.enable_blink.lock(|enabled| *enabled);

        if enabled {
            cx.local.led.toggle();
            defmt::info!("blink");
        }

        Mono::delay(blink::Config::PERIOD_MS.millis()).await;
    }
}

#[task(
    config = [frequency_hz, message],
    local = [hello_timer],
    dependencies = [Defmt],
)]
pub(crate) fn timer_interrupt(cx: timer_interrupt::Context) {
    let _ = cx.local.hello_timer.wait();
    defmt::info!("{}", timer_interrupt::Config::MESSAGE);
}
