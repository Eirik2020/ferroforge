#![no_std]
#![no_main]
use defmt_rtt as _;
use panic_probe as _;
use rtic_monotonics::stm32::prelude::*;
use stm32f4xx_hal::{
    gpio::{Output, PA5, PushPull},
    pac::TIM2,
    rcc::Config,
    timer::{CounterHz, Event},
};
stm32_tim5_monotonic!(Mono, 1000000u32);
#[rtic::app(device = stm32f4xx_hal::pac, dispatchers = [USART1])]
mod app {
    use super::*;
    use stm32f4xx_hal::prelude::*;
    const BLINK_PERIOD_MS: u64 = 2000;
    const TIMER_INTERRUPT_FREQUENCY_HZ: u32 = 1;
    const TIMER_INTERRUPT_MESSAGE: &'static str = "REEEEEEEEEEEEEEEEE";
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
            .start(TIMER_INTERRUPT_FREQUENCY_HZ.Hz())
            .unwrap();
        hello_timer.listen(Event::Update);
        blink::spawn().unwrap();
        (Shared { enable_blink: true }, Local { led, hello_timer })
    }
    #[task(priority = 1, shared = [enable_blink], local = [led])]
    async fn blink(mut cx: blink::Context) -> ! {
        use crate::Mono;
        use fugit::ExtU64 as _;
        loop {
            let enabled = cx.shared.enable_blink.lock(|enabled| *enabled);
            if enabled {
                cx.local.led.toggle();
                defmt::info!("blink");
            }
            Mono::delay(BLINK_PERIOD_MS.millis()).await;
        }
    }
    #[task(binds = TIM2, priority = 2, local = [hello_timer])]
    fn timer_interrupt(cx: timer_interrupt::Context) {
        let _ = cx.local.hello_timer.wait();
        defmt::info!("{}", TIMER_INTERRUPT_MESSAGE);
    }
}
