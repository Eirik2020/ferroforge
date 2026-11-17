//! Nucleo-H753ZI - a Cortex-M7, and a second chip family under the same HAL.
//!
//! What it proves and what it does not:
//!
//! - `blink` and `report` come from `tasks/blinky`, the portable crate,
//!   unchanged. `blink` bounds its LED on embedded-hal 1.0's
//!   `StatefulOutputPin`, which FerroHAL's pins implement on every chip, so
//!   the same definition drives an F4's LED and this one's.
//! - Only `init` is the H7's own: its clock tree has a voltage scale and more
//!   buses than an F4's, so the configuration is spelled out here, as G6
//!   intends for native HAL initialization.
//!
//! LD1, the green user LED, is on PB0 (UM2407 section 6.5).

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

ferroforge::app! {
    device = ferrohal_stm32::device,
    dispatchers = [USART1],

    use rtic_monotonics::systick::prelude::*;

    systick_monotonic!(Mono, 1000);

    use ferroforge_task_blinky::{blink, report};
    use ferrohal_stm32::{
        gpio::{Output, Pin},
        rcc::{AhbPrescaler, ApbPrescaler, Config, Supply, Sysclk, VoltageScale},
    };

    /// The reset clock tree: 64 MHz from the internal oscillator, at the
    /// reset voltage scale (RM0433 section 8.5). A light needs nothing faster.
    const CLOCK_CONFIG: Config = Config {
        hse: None,
        pll1: None,
        sys: Sysclk::Hsi,
        cpu_pre: AhbPrescaler::Div1,
        ahb_pre: AhbPrescaler::Div1,
        apb1_pre: ApbPrescaler::Div1,
        apb2_pre: ApbPrescaler::Div1,
        apb3_pre: ApbPrescaler::Div1,
        apb4_pre: ApbPrescaler::Div1,
        supply: Supply::Ldo,
        voltage_scale: VoltageScale::Scale3,
    };

    #[shared]
    struct Shared {
        blink_enabled: bool,
    }

    #[local]
    struct Local {
        status_led: Pin<'B', 0, Output>,
        blink_count: u32,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let Ok(mut rcc) = cx.device.RCC.freeze(&CLOCK_CONFIG) else {
            defmt::panic!("the clock configuration was refused");
        };
        Mono::start(cx.core.SYST, rcc.clocks().cpu.0);

        let mut gpiob = cx.device.GPIOB.split(&mut rcc);
        let mut status_led = gpiob.p0.into_push_pull_output(&mut gpiob.ctrl);
        status_led.set_low();

        status_blink::spawn().unwrap();
        (
            Shared {
                blink_enabled: true,
            },
            Local {
                status_led,
                blink_count: 0,
            },
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
    async fn status_blink(cx: status_blink::Context) -> !;

    #[task(from = report, priority = 1)]
    async fn telemetry(_cx: telemetry::Context, value: u32);
}
