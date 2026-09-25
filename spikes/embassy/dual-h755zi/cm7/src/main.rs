//! EXPERIMENT: the H755's Cortex-M7 image. RTIC through `ferroforge::app!`
//! on embassy-stm32, as the single-core spike, plus the primary side of
//! embassy's dual-core init and a flight-side reader of the M4's mailbox.
//! Compile-only.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;
use spike_dual_mailbox::{embassy_shared, mailbox};

ferroforge::app! {
    device = embassy_stm32,
    peripherals = false,
    dispatchers = [USART1, USART2],

    use rtic_monotonics::systick::prelude::*;

    systick_monotonic!(Mono, 1000);

    use embassy_stm32::gpio::{Level, Output, Speed};
    use ferroforge_task_blinky::{blink, report};

    #[shared]
    struct Shared {
        blink_enabled: bool,
    }

    #[local]
    struct Local {
        status_led: Output<'static>,
        blink_count: u32,
        last_sequence: u32,
        stale_ticks: u32,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        // Clocks are set up once, here; the M4 waits for this and reuses them.
        let p = embassy_stm32::init_primary(embassy_stm32::Config::default(), crate::embassy_shared::<embassy_stm32::SharedData>());
        Mono::start(cx.core.SYST, 64_000_000);

        let status_led = Output::new(p.PB0, Level::Low, Speed::Low);

        heartbeat::spawn().unwrap();
        rc_input::spawn().unwrap();

        (
            Shared { blink_enabled: true },
            Local { status_led, blink_count: 0, last_sequence: 0, stale_ticks: 0 },
        )
    }

    #[task(
        from = blink,
        priority = 1,
        local = [led = status_led, count = blink_count],
        shared = [enabled = blink_enabled],
        config = [period_ms: u32 = 250],
        spawn = [report = telemetry],
    )]
    async fn heartbeat(cx: heartbeat::Context) -> !;

    #[task(from = report, priority = 1)]
    async fn telemetry(_cx: telemetry::Context, value: u32);

    /// The flight side never waits on the M4: it polls, and a frame that
    /// stops advancing is stale by the M7's own clock.
    #[task(priority = 3, local = [last_sequence, stale_ticks])]
    async fn rc_input(cx: rc_input::Context) {
        loop {
            match crate::mailbox().read() {
                Some((sequence, _channels, _at)) if sequence != *cx.local.last_sequence => {
                    *cx.local.last_sequence = sequence;
                    *cx.local.stale_ticks = 0;
                }
                _ => {
                    *cx.local.stale_ticks = cx.local.stale_ticks.saturating_add(1);
                    if *cx.local.stale_ticks == 100 {
                        defmt::warn!("rc stale: failsafe");
                    }
                }
            }
            Mono::delay(1.millis()).await;
        }
    }
}
