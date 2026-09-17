//! The firmware's authored crate, which is also the binary.
//!
//! No `gen_app/`, no `.ferroforge/`, no generated project. Init and the
//! resources are written here and never move; each task declaration names a
//! task definition and its bindings, and `app!` generates the adapter.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;
use rtic_monotonics::systick::prelude::*;

// The monotonic is this firmware's, declared as in ordinary RTIC. 1 kHz is not
// a free choice: a task declaring `monotonic = Mono` is bounded on
// `Duration<u32, 1, 1000>`, so anything else fails to unify.
systick_monotonic!(Mono, 1000);

ferroforge::app! {
    device = stm32f4xx_hal::pac,
    dispatchers = [USART1],
    monotonic = Mono,

    use ferroforge_task_blinky::{blink, on_tick, report};
    use stm32f4xx_hal::{
        gpio::{Output, PA5, PushPull},
        prelude::*,
        rcc::Config,
    };

    #[shared]
    struct Shared {
        blink_enabled: bool,
    }

    #[local]
    struct Local {
        status_led: PA5<Output<PushPull>>,
        blink_count: u32,
        tick_count: u32,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let mut rcc = cx.device.RCC.freeze(Config::hsi().sysclk(84.MHz()));
        Mono::start(cx.core.SYST, rcc.clocks.sysclk().raw());
        let gpioa = cx.device.GPIOA.split(&mut rcc);
        let mut status_led = gpioa.pa5.into_push_pull_output();
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
        config = [period_ms: u32 = 500],
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
