#![no_std]

use stm32f4xx_hal::{
    gpio::{Output, PA5, PushPull},
    prelude::*,
    rcc::Config,
};

struct Shared {
    blink_enabled: bool,
}

struct Local {
    status_led: PA5<Output<PushPull>>,
    blink_count: u32,
}

#[ferroforge::init]
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
