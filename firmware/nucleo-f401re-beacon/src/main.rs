//! A second application on the same board, to prove reuse.
//!
//! Nothing in `tasks/blinky` changes to support this. `blink` is instantiated
//! twice here with different names, pins, counters, gates and periods; `on_tick`
//! binds `TIM3` where the other firmware binds `TIM2`, which is the point of a
//! definition that never names an interrupt itself.
//!
//! PA5 is the board's LD2. PB0 is an ordinary header pin with no LED on it: this
//! firmware is evidence that the composition checks and links, not that anything
//! was observed blinking.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

ferroforge::compose! {
    device = stm32f4xx_hal::pac,
    dispatchers = [USART2, USART6],
    monotonic_hz = 1000,

    use ferroforge_task_blinky::{blink, on_tick, report};
    use stm32f4xx_hal::{
        gpio::{Output, PA5, PB0, PushPull},
        prelude::*,
        rcc::Config,
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

    #[task(
        from = on_tick,
        binds = TIM3,
        priority = 3,
        local = [ticks = pulse_count],
    )]
    fn pulse(cx: pulse::Context);
}
