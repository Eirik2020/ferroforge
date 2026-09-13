#![no_std]
#![no_main]
use defmt_rtt as _;
use panic_probe as _;
use rtic_monotonics::systick::prelude::*;
systick_monotonic!(__FerroforgeMono, 1000u32);
mod __ferroforge_sources {
    pub mod __ferroforge_module_root {
        pub use embedded_hal::digital::StatefulOutputPin;
        pub use fugit::ExtU32 as _;
        pub fn increment(value: &mut u32) {
            *value = value.wrapping_add(1);
        }
    }
    #[rtic::app(device = stm32f4xx_hal::pac, dispatchers = [USART1])]
    mod app {
        use super::*;
        use rtic_monotonics::systick::prelude::*;
        use stm32f4xx_hal::{
            gpio::{Output, PA5, PushPull},
            prelude::*, rcc::Config,
        };
        mod __ferroforge_config {
            pub(super) mod status_blink {
                pub(crate) const PERIOD_MS: u32 = 500;
            }
        }
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
            crate::__FerroforgeMono::start(cx.core.SYST, rcc.clocks.sysclk().raw());
            let gpioa = cx.device.GPIOA.split(&mut rcc);
            let mut status_led = gpioa.pa5.into_push_pull_output();
            status_led.set_low();
            status_blink::spawn().unwrap();
            (
                Shared { blink_enabled: true },
                Local {
                    status_led,
                    blink_count: 0,
                },
            )
        }
        #[task(
            priority = 1,
            local = [blink_count,
            status_led],
            shared = [blink_enabled]
        )]
        async fn status_blink(mut cx: status_blink::Context) -> ! {
            #[allow(unused_imports)]
            use crate::__ferroforge_sources::__ferroforge_module_root::*;
            loop {
                if cx.shared.blink_enabled.lock(|enabled| *enabled) {
                    let _ = StatefulOutputPin::toggle(&mut *cx.local.status_led);
                    increment(cx.local.blink_count);
                    let _: Result<(), u32> = telemetry::spawn(*cx.local.blink_count);
                }
                crate::__FerroforgeMono::delay(
                        __ferroforge_config::status_blink::PERIOD_MS.millis(),
                    )
                    .await;
            }
        }
        #[task(priority = 1)]
        async fn telemetry(_cx: telemetry::Context, value: u32) {
            #[allow(unused_imports)]
            use crate::__ferroforge_sources::__ferroforge_module_root::*;
            defmt::info!("blink count={=u32}", value);
        }
    }
}
