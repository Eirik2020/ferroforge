//! EXPERIMENT: embassy-stm32 as the HAL under a FerroForge `app!`, on the
//! Nucleo-H753ZI. Compile-only; nothing here has been flashed.
//!
//! What it tries:
//!
//! - `blink` from `tasks/blinky`, unchanged. `nucleo-h753zi` cannot select it,
//!   because `stm32h7xx-hal` 0.16 implements only embedded-hal 0.2; embassy's
//!   `Output` implements 1.0.
//! - `device = embassy_stm32` with `peripherals = false`: RTIC takes the
//!   interrupt enum and `NVIC_PRIO_BITS` from embassy, and init calls
//!   `embassy_stm32::init` itself.
//! - An embassy async DMA UART read inside an RTIC software task, through the
//!   portable `receive` task, which bounds on `embedded_io_async::Read`.
//! - embassy-time (TIM2 driver) and the RTIC SysTick monotonic side by side:
//!   `blink` waits on `Mono`, `receive` and `pace` on embassy-time.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

ferroforge::app! {
    device = embassy_stm32,
    peripherals = false,
    // Not TIM2 (embassy's time driver owns it) and not USART3 or DMA1_STR0
    // (bound to embassy's handlers below).
    dispatchers = [USART1, USART2],

    use rtic_monotonics::systick::prelude::*;

    systick_monotonic!(Mono, 1000);

    use embassy_stm32::{
        bind_interrupts,
        gpio::{Level, Output, Speed},
        peripherals,
        usart::{self, RingBufferedUartRx, UartRx},
    };
    use ferroforge_task_blinky::{blink, report};
    use ferroforge_task_spike_embassy_async::{pace, receive};

    // Embassy's own interrupt binding. It defines the `USART3` and
    // `DMA1_STR0` vectors, so RTIC must not bind or dispatch on them. An
    // RTIC hardware task claiming one would fail to link as a duplicate
    // symbol, not on the authored line.
    bind_interrupts!(struct Irqs {
        USART3 => usart::InterruptHandler<peripherals::USART3>;
        DMA1_STREAM0 => embassy_stm32::dma::InterruptHandler<peripherals::DMA1_CH0>;
    });

    #[shared]
    struct Shared {
        blink_enabled: bool,
    }

    #[local]
    struct Local {
        status_led: Output<'static>,
        blink_count: u32,
        uart_rx: RingBufferedUartRx<'static>,
    }

    #[init(local = [dma_buffer: [u8; 64] = [0; 64]])]
    fn init(cx: init::Context) -> (Shared, Local) {
        let p = embassy_stm32::init(embassy_stm32::Config::default());

        // HSI at 64 MHz with the default config.
        Mono::start(cx.core.SYST, 64_000_000);

        let status_led = Output::new(p.PB0, Level::Low, Speed::Low);

        let uart_rx = UartRx::new(p.USART3, p.PD9, p.DMA1_CH0, Irqs, usart::Config::default())
            .unwrap()
            .into_ring_buffered(cx.local.dma_buffer);

        heartbeat::spawn().unwrap();
        uart::spawn().unwrap();
        control_loop::spawn().unwrap();

        (
            Shared { blink_enabled: true },
            Local {
                status_led,
                blink_count: 0,
                uart_rx,
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
    async fn heartbeat(cx: heartbeat::Context) -> !;

    #[task(from = report, priority = 1)]
    async fn telemetry(_cx: telemetry::Context, value: u32);

    #[task(
        from = receive,
        priority = 2,
        local = [rx = uart_rx],
        config = [timeout_ms: u32 = 100],
        spawn = [report = telemetry],
    )]
    async fn uart(cx: uart::Context) -> !;

    #[task(from = pace, priority = 2, config = [period_us: u32 = 500])]
    async fn control_loop(cx: control_loop::Context) -> !;
}
