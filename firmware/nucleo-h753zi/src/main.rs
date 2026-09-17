//! Nucleo-H753ZI - a Cortex-M7, and the first board here on a different HAL.
//!
//! What it proves and what it does not:
//!
//! - `report` comes from `tasks/blinky`, the portable crate, unchanged. A task
//!   that names no HAL really is reusable across HALs.
//! - `on_timer` comes from `tasks/stm32h7-timer`, which is the F4 timer task's
//!   counterpart. The work is identical and the API is not, which is why such a
//!   task is HAL-specific rather than portable.
//! - `blink` is **not** used, and cannot be: it bounds on
//!   `embedded_hal::digital::StatefulOutputPin` from embedded-hal 1.0, and
//!   `stm32h7xx-hal` 0.16 still implements only 0.2. The LED is driven here
//!   instead. That is an ecosystem gap, not a FerroForge one, and it is the real
//!   limit on what "portable across all hardware" means today.
//!
//! LD1, the green user LED, is on PB0. Nothing here has been flashed.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;
use rtic_monotonics::systick::prelude::*;

systick_monotonic!(Mono, 1000);

ferroforge::app! {
    device = stm32h7xx_hal::pac,
    dispatchers = [USART1],
    monotonic = Mono,

    use ferroforge_task_blinky::report;
    use ferroforge_task_stm32h7_timer::on_timer;
    // The firmware's own task below reads the clock, so the trait and the
    // duration extension have to be in scope here. `app!` no longer imports
    // them for you - the monotonic is yours, as in ordinary RTIC.
    use rtic_monotonics::{Monotonic as _, fugit::ExtU32 as _};
    use stm32h7xx_hal::{
        gpio::{Output, PushPull, gpiob::PB0},
        pac::TIM2,
        prelude::*,
        timer::{Event, Timer},
    };

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        // Driven from `heartbeat` below rather than by `blink`, because this
        // HAL's pins do not implement embedded-hal 1.0.
        status_led: PB0<Output<PushPull>>,
        loop_timer: Timer<TIM2>,
        loop_count: u32,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        // An H7 needs its core voltage set before the PLL is configured, which
        // is why this init is longer than the F4's. Ordinary HAL calls, in the
        // firmware, exactly as G6 intends.
        let pwr = cx.device.PWR.constrain();
        let pwrcfg = pwr.freeze();

        let rcc = cx.device.RCC.constrain();
        let ccdr = rcc.sys_ck(200.MHz()).freeze(pwrcfg, &cx.device.SYSCFG);

        Mono::start(cx.core.SYST, ccdr.clocks.sys_ck().raw());

        let gpiob = cx.device.GPIOB.split(ccdr.peripheral.GPIOB);
        let mut status_led = gpiob.pb0.into_push_pull_output();
        status_led.set_low();

        let mut loop_timer = cx
            .device
            .TIM2
            .timer(500.Hz(), ccdr.peripheral.TIM2, &ccdr.clocks);
        loop_timer.listen(Event::TimeOut);

        heartbeat::spawn().unwrap();

        (
            Shared {},
            Local {
                status_led,
                loop_timer,
                loop_count: 0,
            },
        )
    }

    /// The LED, driven locally. On a HAL that implemented embedded-hal 1.0 this
    /// would be an instance of `blink` from `tasks/blinky` instead.
    #[task(priority = 1, local = [status_led])]
    async fn heartbeat(cx: heartbeat::Context) {
        loop {
            cx.local.status_led.toggle();
            Mono::delay(250.millis()).await;
        }
    }

    #[task(from = report, priority = 1)]
    async fn telemetry(_cx: telemetry::Context, value: u32);

    #[task(
        from = on_timer,
        binds = TIM2,
        priority = 3,
        local = [timer = loop_timer, elapsed = loop_count],
        spawn = [elapsed = telemetry],
    )]
    fn loop_tick(cx: loop_tick::Context);
}
