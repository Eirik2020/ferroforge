//! The call-through equivalent of `systems/nucleo-f401re/gen_app`.
//!
//! Same behaviour, same target, same RTIC declarations. The difference is that
//! the task bodies are not copied in: each handler builds the reusable
//! context from its own RTIC context and calls the library.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;
use rtic_monotonics::systick::prelude::*;

systick_monotonic!(Mono, 1000);

#[rtic::app(device = stm32f4xx_hal::pac, dispatchers = [USART1])]
mod app {
    use super::Mono;
    use blinky_tasks::{BlinkContext, blink, report};
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
            },
        )
    }

    // The generated handler: bind resources, supply configuration, and hand the
    // outgoing alias over as a closure onto the real instance.
    #[task(priority = 1, local = [blink_count, status_led], shared = [blink_enabled])]
    async fn status_blink(cx: status_blink::Context) -> ! {
        blink::<_, _, Mono, _, 500, 5>(
            BlinkContext {
                led: cx.local.status_led,
                count: cx.local.blink_count,
                enabled: cx.shared.blink_enabled,
            },
            |value| telemetry::spawn(value),
        )
        .await
    }

    #[task(priority = 1)]
    async fn telemetry(_cx: telemetry::Context, value: u32) {
        report(value).await
    }
}
