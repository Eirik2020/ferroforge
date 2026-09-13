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
            pub(super) mod heartbeat {
                pub(crate) const PERIOD_MS: u32 = 125;
            }
        }
        #[shared]
        struct Shared {
            heartbeat_enabled: bool,
        }
        #[local]
        struct Local {
            activity_led: PA5<Output<PushPull>>,
            pulse_count: u32,
        }
        #[init]
        fn init(cx: init::Context) -> (Shared, Local) {
            let mut rcc = cx.device.RCC.freeze(Config::hsi().sysclk(84.MHz()));
            crate::__FerroforgeMono::start(cx.core.SYST, rcc.clocks.sysclk().raw());
            let gpioa = cx.device.GPIOA.split(&mut rcc);
            let mut activity_led = gpioa.pa5.into_push_pull_output();
            activity_led.set_low();
            heartbeat::spawn().unwrap();
            (
                Shared { heartbeat_enabled: true },
                Local {
                    activity_led,
                    pulse_count: 0,
                },
            )
        }
        #[task(
            priority = 1,
            local = [pulse_count,
            activity_led],
            shared = [heartbeat_enabled]
        )]
        async fn heartbeat(mut cx: heartbeat::Context) -> ! {
            #[allow(unused_imports)]
            use crate::__ferroforge_sources::__ferroforge_module_root::*;
            loop {
                if cx.shared.heartbeat_enabled.lock(|enabled| *enabled) {
                    let _ = StatefulOutputPin::toggle(&mut *cx.local.activity_led);
                    increment(cx.local.pulse_count);
                    let _: Result<(), u32> = diagnostics::spawn(*cx.local.pulse_count);
                }
                crate::__FerroforgeMono::delay(
                        __ferroforge_config::heartbeat::PERIOD_MS.millis(),
                    )
                    .await;
            }
        }
        #[task(priority = 1)]
        async fn diagnostics(_cx: diagnostics::Context, value: u32) {
            #[allow(unused_imports)]
            use crate::__ferroforge_sources::__ferroforge_module_root::*;
            defmt::info!("blink count={=u32}", value);
        }
    }
}
