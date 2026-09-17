//! Foxeer F405 V2 - a flight controller, and the first board here that is not
//! a Nucleo.
//!
//! It selects the same definitions as the Nucleo firmwares without either task
//! crate changing, which is the point: `tasks/blinky` is portable and
//! `tasks/stm32f4-timer` is HAL-specific but not board-specific.
//!
//! **Two board facts are unverified and marked below:** which pin drives the
//! status LED, and whether to run from the on-board crystal. This runs from the
//! internal oscillator so it does not depend on a crystal frequency, and names
//! the LED pin in one place so it is one line to correct.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;
use rtic_monotonics::systick::prelude::*;

systick_monotonic!(Mono, 1000);

ferroforge::app! {
    device = stm32f4xx_hal::pac,
    dispatchers = [USART1],
    monotonic = Mono,

    use ferroforge_task_blinky::{blink, report};
    use ferroforge_task_stm32f4_timer::on_timer;
    use stm32f4xx_hal::{
        gpio::{Output, PB5, PushPull},
        pac::TIM3,
        prelude::*,
        rcc::Config,
        timer::{CounterUs, Event},
    };

    #[shared]
    struct Shared {
        armed: bool,
    }

    #[local]
    struct Local {
        // UNVERIFIED: the status LED pin on this board. Change the type here and
        // the `gpiob.pb5` line below together if it is wrong; nothing else in
        // this firmware depends on which pin it is.
        status_led: PB5<Output<PushPull>>,
        blink_count: u32,
        loop_timer: CounterUs<TIM3>,
        loop_count: u32,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        // The internal oscillator, deliberately: an F405 flight controller's
        // crystal is a board fact, and HSI runs on any of them. Switch to
        // `Config::hse(..)` once the crystal frequency is known.
        let mut rcc = cx.device.RCC.freeze(Config::hsi().sysclk(84.MHz()));
        Mono::start(cx.core.SYST, rcc.clocks.sysclk().raw());

        let gpiob = cx.device.GPIOB.split(&mut rcc);
        let mut status_led = gpiob.pb5.into_push_pull_output();
        status_led.set_low();

        // A periodic loop tick, which is what a flight controller actually
        // wants this timer for.
        let mut loop_timer = cx.device.TIM3.counter_us(&mut rcc);
        loop_timer.start(2.millis().into()).unwrap();
        loop_timer.listen(Event::Update);

        heartbeat::spawn().unwrap();

        (
            Shared { armed: false },
            Local {
                status_led,
                blink_count: 0,
                loop_timer,
                loop_count: 0,
            },
        )
    }

    #[task(
        from = blink,
        priority = 1,
        local = [led = status_led, count = blink_count],
        shared = [enabled = armed],
        config = [period_ms: u32 = 100],
        spawn = [report = telemetry],
    )]
    async fn heartbeat(cx: heartbeat::Context) -> !;

    #[task(from = report, priority = 1)]
    async fn telemetry(_cx: telemetry::Context, value: u32);

    #[task(
        from = on_timer,
        binds = TIM3,
        priority = 3,
        local = [timer = loop_timer, elapsed = loop_count],
        spawn = [elapsed = telemetry],
    )]
    fn loop_tick(cx: loop_tick::Context);
}
