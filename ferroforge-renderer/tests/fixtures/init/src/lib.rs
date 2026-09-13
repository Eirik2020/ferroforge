#![no_std]

use stm32f4xx_hal::{
    gpio::{Output, PA5, PushPull},
    prelude::*,
    rcc::Config,
};

struct Shared {
    enabled: bool,
}

struct Local {
    led: PA5<Output<PushPull>>,
}

#[ferroforge::init]
fn init(cx: init::Context) -> (Shared, Local) {
    let mut rcc = cx.device.RCC.freeze(Config::hsi().sysclk(84.MHz()));
    Mono::start(cx.core.SYST, rcc.clocks.sysclk().raw());

    let gpioa = cx.device.GPIOA.split(&mut rcc);
    let mut led = gpioa.pa5.into_push_pull_output();
    led.set_low();

    status::spawn().unwrap();
    telemetry::spawn(7).unwrap();

    (Shared { enabled: true }, Local { led })
}

