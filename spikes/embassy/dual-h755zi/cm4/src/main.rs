//! EXPERIMENT: the H755's Cortex-M4 image. Plain embassy - its executor, an
//! async DMA UART and embassy-time - running the receiver side and handing
//! each frame to the M7 through the SRAM4 mailbox. Compile-only.
//!
//! Nothing stops this core from touching any peripheral: `init_secondary`
//! hands it the same full `Peripherals` set the M7 gets. Which core owns
//! which peripheral is a convention here, not something the compiler checks.

#![no_std]
#![no_main]

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::{
    SharedData, bind_interrupts, peripherals,
    usart::{self, UartRx},
};
use embassy_time::Instant;
use embedded_io_async::Read as _;
use panic_probe as _;
use spike_dual_mailbox::{embassy_shared, mailbox};

bind_interrupts!(struct Irqs {
    USART3 => usart::InterruptHandler<peripherals::USART3>;
    DMA1_STREAM0 => embassy_stm32::dma::InterruptHandler<peripherals::DMA1_CH0>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // Waits for the M7 to finish clock setup, then shares its clock tree.
    let p = embassy_stm32::init_secondary(embassy_shared::<SharedData>());
    mailbox().reset();

    let mut dma_buffer = [0u8; 128];
    let mut rx = UartRx::new(p.USART3, p.PD9, p.DMA1_CH0, Irqs, usart::Config::default())
        .unwrap()
        .into_ring_buffered(&mut dma_buffer);

    let mut frame = [0u8; 25];
    loop {
        if rx.read_exact(&mut frame).await.is_err() {
            defmt::warn!("rx error");
            continue;
        }
        // Stand-in for SBUS decoding: eight channels from the first bytes.
        let mut channels = [0u16; 8];
        for (index, channel) in channels.iter_mut().enumerate() {
            *channel = u16::from_le_bytes([frame[1 + 2 * index], frame[2 + 2 * index]]);
        }
        mailbox().publish(&channels, Instant::now().as_micros() as u32);
    }
}
