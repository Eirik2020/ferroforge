//! The firmware's authored crate, which is also the binary.
//!
//! No `gen_app/`, no `.ferroforge/`, no generated project. Init and the
//! resources are written here and never move; each task declaration names a
//! task definition and its bindings, and `app!` generates the adapter.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

ferroforge::app! {
    device = ferrohal_stm32::device,
    dispatchers = [USART1],

    use rtic_monotonics::systick::prelude::*;

    // The monotonic is this firmware's, declared as in ordinary RTIC. 1 kHz is
    // not a free choice: a task declaring `monotonic = Mono` is bounded on
    // `Duration<u32, 1, 1000>`, so anything else fails to unify.
    systick_monotonic!(Mono, 1000);

    use ferroforge_task_blinky::{blink, on_tick, report};
    use ferrohal_stm32::{
        gpio::{Output, Pin},
        rcc::Config,
    };

    #[shared]
    struct Shared {
        blink_enabled: bool,
    }

    #[local]
    struct Local {
        status_led: Pin<'A', 5, Output>,
        blink_count: u32,
        tick_count: u32,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        // The reset clock tree, 16 MHz from the internal oscillator: a light
        // needs nothing faster.
        let Ok(mut rcc) = cx.device.RCC.freeze(&Config::hsi()) else {
            defmt::panic!("the clock configuration was refused");
        };
        Mono::start(cx.core.SYST, rcc.clocks().cpu.0);
        // LD2, the Nucleo-64's green user LED (UM1724 section 6.4).
        let mut gpioa = cx.device.GPIOA.split(&mut rcc);
        let mut status_led = gpioa.p5.into_push_pull_output(&mut gpioa.ctrl);
        status_led.set_low();
        status_blink::spawn().unwrap();
        (
            Shared {
                blink_enabled: true,
            },
            Local {
                status_led,
                blink_count: 0,
                tick_count: 0,
            },
        )
    }

    #[task(
        from = blink,
        priority = 1,
        local = [led = status_led, count = blink_count],
        shared = [enabled = blink_enabled],
        config = [period_ms: u32 = 2000],
        spawn = [report = telemetry],
    )]
    async fn status_blink(cx: status_blink::Context) -> !;

    #[task(from = report, priority = 1)]
    async fn telemetry(_cx: telemetry::Context, value: u32);

    // A hardware task: synchronous, and bound to a real interrupt here rather
    // than in the reusable definition.
    #[task(
        from = on_tick,
        binds = TIM2,
        priority = 2,
        local = [ticks = tick_count],
    )]
    fn tick(cx: tick::Context);
}
