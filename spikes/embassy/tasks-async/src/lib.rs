//! EXPERIMENT: reusable tasks written against embassy-time and an async I/O
//! trait. Neither needs `monotonic = Mono`: embassy-time is one global clock
//! whose tick rate the firmware picks with a Cargo feature, so the task has no
//! clock type parameter and no fixed 1 kHz profile to match.

#![no_std]

use embassy_time::{Duration, Instant, Timer, with_timeout};
use embedded_io_async::Read;

/// Reads bytes from any `embedded_io_async::Read` - an embassy DMA ring-buffered
/// UART here - with a receive timeout, and reports the byte count.
#[ferroforge::task(
    bounds = [rx: Read],
    local = [rx],
    config = [timeout_ms: u32],
    spawn = [report(count: u32)],
)]
pub async fn receive(mut cx: receive::Context) -> ! {
    let mut buffer = [0u8; 32];
    loop {
        let timeout = Duration::from_millis(CONFIG::TIMEOUT_MS as u64);
        match with_timeout(timeout, cx.local.rx.read(&mut buffer)).await {
            Ok(Ok(count)) => {
                let _: Result<(), u32> = cx.spawn.report(count as u32);
            }
            Ok(Err(_)) => defmt::warn!("rx error"),
            Err(_) => defmt::warn!("rx timeout"),
        }
    }
}

/// A periodic loop paced by embassy-time at whatever tick rate the firmware
/// selected, measuring its own jitter with `Instant::now()`.
#[ferroforge::task(config = [period_us: u32])]
pub async fn pace(_cx: pace::Context) -> ! {
    let period = Duration::from_micros(CONFIG::PERIOD_US as u64);
    let mut next = Instant::now() + period;
    loop {
        Timer::at(next).await;
        let late = Instant::now().saturating_duration_since(next);
        if late > Duration::from_micros(50) {
            defmt::warn!("late by {=u64} us", late.as_micros());
        }
        next += period;
    }
}
