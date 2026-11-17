//! A second application on the same board, selecting the same definitions as
//! the first, and the one that exercises groups.
//!
//! `blinky`'s `light` group is instantiated three times - twice inside its own
//! `lights` group and once on its own - with different names, pins, counters,
//! gates and periods. `pulse_light`, from another crate, nests `light` once
//! more beside a hardware task bound to `TIM3` here: the definition names no
//! interrupt, so which vector serves it is the composition's choice.
//!
//! Nothing in the task crates names a HAL. `TIM3`'s vector is pended from
//! software by `status`, so the hardware task runs on a bare board without
//! TIM3 itself ever being started.
//!
//! PA5 is the board's LD2 (UM1724 section 6.4). PB0, PA8 and PA6 are ordinary
//! header pins with no LED on them; fit one to watch them.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

ferroforge::app! {
    device = ferrohal_stm32::device,
    // A dispatcher is an interrupt vector RTIC borrows for software tasks, so
    // a peripheral raising the same vector would land in the dispatcher
    // instead of its own handler. SPI1 and USART2 are unused on this board.
    dispatchers = [USART2, SPI1],

    use rtic_monotonics::systick::prelude::*;

    systick_monotonic!(Mono, 1000);

    use ferroforge_task_blinky as blinky;
    use ferroforge_task_pulse::pulse_light;
    use ferrohal_stm32::{
        device::Interrupt,
        gpio::{Output, Pin},
        rcc::Config,
    };

    #[shared]
    struct Shared {
        heartbeat_enabled: bool,
        beacon_enabled: bool,
    }

    #[local]
    struct Local {
        heartbeat_led: Pin<'A', 5, Output>,
        beacon_led: Pin<'B', 0, Output>,
        heartbeat_count: u32,
        beacon_count: u32,
        spare_led: Pin<'A', 8, Output>,
        spare_count: u32,
        pulse_led: Pin<'A', 6, Output>,
        pulse_blinks: u32,
        pulse_count: u32,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let Ok(mut rcc) = cx.device.RCC.freeze(&Config::hsi()) else {
            defmt::panic!("the clock configuration was refused");
        };
        Mono::start(cx.core.SYST, rcc.clocks().cpu.0);

        let mut gpioa = cx.device.GPIOA.split(&mut rcc);
        let mut gpiob = cx.device.GPIOB.split(&mut rcc);
        let mut heartbeat_led = gpioa.p5.into_push_pull_output(&mut gpioa.ctrl);
        let mut beacon_led = gpiob.p0.into_push_pull_output(&mut gpiob.ctrl);
        let mut spare_led = gpioa.p8.into_push_pull_output(&mut gpioa.ctrl);
        let mut pulse_led = gpioa.p6.into_push_pull_output(&mut gpioa.ctrl);
        heartbeat_led.set_low();
        beacon_led.set_low();
        spare_led.set_low();
        pulse_led.set_low();

        lights::heartbeat_blink::spawn().unwrap();
        lights::beacon_blink::spawn().unwrap();
        spare::blink::spawn().unwrap();
        pulse::blink::spawn().unwrap();
        status::spawn().unwrap();

        (
            Shared {
                heartbeat_enabled: true,
                beacon_enabled: true,
            },
            Local {
                heartbeat_led,
                beacon_led,
                heartbeat_count: 0,
                beacon_count: 0,
                spare_led,
                spare_count: 0,
                pulse_led,
                pulse_blinks: 0,
                pulse_count: 0,
            },
        )
    }

    // One group holding the same group of two twice, as `heartbeat` and
    // `beacon` - a light and the announcer it is wired to inside the group -
    // plus a reporter merged in unprefixed. Every name a copy has is prefixed
    // with its copy's name, so the firmware's resources bind by bare name. The
    // two copies differ in every binding a composition controls - name,
    // priority, resources, gate, period and label - and share only the
    // definitions; each announces itself, so the log tells them apart.
    #[group(
        from = blinky::lights,
        local = [heartbeat_led, heartbeat_count, beacon_led, beacon_count],
        shared = [heartbeat_enabled, beacon_enabled],
        config = [
            heartbeat_period_ms: u32 = 250,
            heartbeat_label: &'static str = "heartbeat",
            beacon_period_ms: u32 = 1000,
            beacon_label: &'static str = "beacon",
        ],
    )]
    mod lights {
        #[task(priority = 1)]
        async fn heartbeat_blink;

        #[task(priority = 1)]
        async fn heartbeat_announce;

        #[task(priority = 2)]
        async fn beacon_blink;

        #[task(priority = 2)]
        async fn beacon_announce;

        #[task(priority = 1)]
        async fn report;
    }

    // The group `lights` holds twice, selected a third time on its own. A
    // group is a definition like a task is, so a firmware may select it as
    // often as it has resources for; the module's name keeps each selection's
    // tasks apart. PA8 is Arduino D7.
    #[group(
        from = blinky::light,
        local = [led = spare_led, count = spare_count],
        shared = [enabled = heartbeat_enabled],
        config = [period_ms: u32 = 500, label: &'static str = "spare"],
    )]
    mod spare {
        #[task(priority = 1)]
        async fn blink;

        #[task(priority = 1)]
        async fn announce;
    }

    // A group of groups across crates: `pulse_light` is `blinky`'s `light`
    // plus a hardware task, wired to the light's announcer inside the `pulse`
    // crate. This binds the union of their resources once and declares each
    // of the three tasks as RTIC declares a task. PA6 is Arduino D12.
    #[group(
        from = pulse_light,
        local = [led = pulse_led, count = pulse_blinks, pulses = pulse_count],
        shared = [enabled = beacon_enabled],
        config = [period_ms: u32 = 100, label: &'static str = "pulse"],
    )]
    mod pulse {
        #[task(binds = TIM3, priority = 3)]
        fn on_pulse;

        #[task(priority = 1)]
        async fn blink;

        #[task(priority = 2)]
        async fn announce;
    }

    /// Proof of life, once a second, and the pulse source: pending `TIM3`'s
    /// vector runs `pulse::on_pulse` with no timer behind it.
    #[task(priority = 1, local = [seconds: u32 = 0])]
    async fn status(cx: status::Context) {
        loop {
            *cx.local.seconds = cx.local.seconds.wrapping_add(1);
            defmt::info!("alive {=u32}s", *cx.local.seconds);
            rtic::pend(Interrupt::TIM3);
            Mono::delay(1000u64.millis()).await;
        }
    }
}
