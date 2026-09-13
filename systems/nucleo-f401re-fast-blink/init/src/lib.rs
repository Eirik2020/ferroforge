#![no_std]

use stm32f4xx_hal::{
    gpio::{Output, PA5, PushPull},
    prelude::*,
    rcc::Config,
};

struct Shared {
    heartbeat_enabled: bool,
}

struct Local {
    activity_led: PA5<Output<PushPull>>,
    pulse_count: u32,
}

#[ferroforge::init]
fn init(cx: init::Context) -> (Shared, Local) {
    let mut rcc = cx.device.RCC.freeze(Config::hsi().sysclk(84.MHz()));
    Mono::start(cx.core.SYST, rcc.clocks.sysclk().raw());

    let gpioa = cx.device.GPIOA.split(&mut rcc);
    let mut activity_led = gpioa.pa5.into_push_pull_output();
    activity_led.set_low();

    heartbeat::spawn().unwrap();

    (
        Shared {
            heartbeat_enabled: true,
        },
        Local {
            activity_led,
            pulse_count: 0,
        },
    )
}
