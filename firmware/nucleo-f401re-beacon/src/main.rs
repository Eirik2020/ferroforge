//! A second application on the same board, to prove reuse.
//!
//! No task crate changes to support this. `blink` is instantiated twice with
//! different names, pins, counters, gates and periods, and the HAL-specific
//! `on_timer` is bound to `TIM3` here - the definition names no interrupt, so
//! which line serves it is the composition's choice.
//!
//! PA5 is the board's LD2. PB0 is an ordinary header pin with no LED on it: this
//! firmware is evidence that the composition checks and links, not that anything
//! was observed blinking.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;
use rtic_monotonics::systick::prelude::*;

systick_monotonic!(Mono, 1000);

ferroforge::app! {
    device = stm32f4xx_hal::pac,
    dispatchers = [USART2, USART6],
    monotonic = Mono,

    use ferroforge_task_blinky::{blink, report};
    use ferroforge_task_stm32f4_timer::on_timer;
    use stm32f4xx_hal::{
        gpio::{Output, PA5, PB0, PushPull},
        pac::TIM3,
        prelude::*,
        rcc::Config,
        timer::{CounterUs, Event},
    };

    #[shared]
    struct Shared {
        heartbeat_enabled: bool,
        beacon_enabled: bool,
    }

    #[local]
    struct Local {
        heartbeat_led: PA5<Output<PushPull>>,
        beacon_led: PB0<Output<PushPull>>,
        heartbeat_count: u32,
        beacon_count: u32,
        pulse_timer: CounterUs<TIM3>,
        pulse_count: u32,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let mut rcc = cx.device.RCC.freeze(Config::hsi().sysclk(84.MHz()));
        Mono::start(cx.core.SYST, rcc.clocks.sysclk().raw());

        let gpioa = cx.device.GPIOA.split(&mut rcc);
        let gpiob = cx.device.GPIOB.split(&mut rcc);
        let mut heartbeat_led = gpioa.pa5.into_push_pull_output();
        let mut beacon_led = gpiob.pb0.into_push_pull_output();
        heartbeat_led.set_low();
        beacon_led.set_low();

        // The HAL-specific task reads and clears this timer's flags; init owns
        // everything else about it.
        let mut pulse_timer = cx.device.TIM3.counter_us(&mut rcc);
        pulse_timer.start(500.millis().into()).unwrap();
        pulse_timer.listen(Event::Update);

        heartbeat::spawn().unwrap();
        beacon::spawn().unwrap();

        (
            Shared {
                heartbeat_enabled: true,
                beacon_enabled: false,
            },
            Local {
                heartbeat_led,
                beacon_led,
                heartbeat_count: 0,
                beacon_count: 0,
                pulse_timer,
                pulse_count: 0,
            },
        )
    }

    // Two instances of one definition. They differ in every binding a
    // composition controls - name, priority, resources, gate and period - and
    // share only the definition itself and the telemetry task they report to.
    #[task(
        from = blink,
        priority = 1,
        local = [led = heartbeat_led, count = heartbeat_count],
        shared = [enabled = heartbeat_enabled],
        config = [period_ms: u32 = 250],
        spawn = [report = telemetry],
    )]
    async fn heartbeat(cx: heartbeat::Context) -> !;

    #[task(
        from = blink,
        priority = 2,
        local = [led = beacon_led, count = beacon_count],
        shared = [enabled = beacon_enabled],
        config = [period_ms: u32 = 1000],
        spawn = [report = telemetry],
    )]
    async fn beacon(cx: beacon::Context) -> !;

    #[task(from = report, priority = 1)]
    async fn telemetry(_cx: telemetry::Context, value: u32);

    // The HAL-specific hardware task. Unlike a portable definition it can read
    // and clear the peripheral's update flag, which is what makes the binding
    // actually work rather than merely compile.
    #[task(
        from = on_timer,
        binds = TIM3,
        priority = 3,
        local = [timer = pulse_timer, elapsed = pulse_count],
        spawn = [elapsed = telemetry],
    )]
    fn pulse(cx: pulse::Context);
}
