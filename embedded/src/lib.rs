#![no_std]
#![allow(dead_code)]

use ferroforge::app;

mod dependencies;

#[ferroforge::firmware]
use stm32f4xx_hal::{
    gpio::{Output, PA5, PushPull},
    pac::TIM2,
    rcc::Config,
    timer::{CounterHz, Event},
};

app! {
    dependency_registry = crate::dependencies,
    target = {
        mcu = STM32F401RET6,
        hal = stm32f4xx_hal,
    },
    dispatchers = [USART1],
    monotonic = Mono {
        source = Timer(TIM5),
        tick_hz = 1_000_000,
    },

    mod app {
        mod tasks;

        #[shared]
        struct Shared {
            enable_blink: bool,
        }

        #[local]
        struct Local {
            led: PA5<Output<PushPull>>,
            hello_timer: CounterHz<TIM2>,
        }

        #[init]
        fn init(cx: init::Context) -> (Shared, Local) {
            let mut rcc = cx.device.RCC.freeze(Config::hsi().sysclk(84.MHz()));
            Mono::start(rcc.clocks.timclk1().raw());

            let gpioa = cx.device.GPIOA.split(&mut rcc);
            let mut led = gpioa.pa5.into_push_pull_output();
            led.set_low();

            let mut hello_timer = cx.device.TIM2.counter_hz(&mut rcc);
            hello_timer
                .start(timer_interrupt::Config::FREQUENCY_HZ.Hz())
                .unwrap();
            hello_timer.listen(Event::Update);

            blink::spawn().unwrap();

            (
                Shared { enable_blink: true },
                Local { led, hello_timer },
            )
        }

        // These values exist only to type-check the reusable embedded source.
        // The host composition will own the values used by rendered firmware.
        blink {
            priority = 1,
            config = {
                period_ms: u64 = 500,
            },
        },

        timer_interrupt {
            binds = TIM2,
            priority = 2,
            config = {
                frequency_hz: u32 = 1,
                message: &'static str = "Hello World",
            },
        },
    }
}

pub use app::APPLICATION;
