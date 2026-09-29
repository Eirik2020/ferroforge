//! An SBUS receiver, built from another crate's group.
//!
//! SBUS is the DMA UART's receive half plus a decoder: the UART delivers a
//! frame and says how long it is, and `decode` turns its 25 bytes into sixteen
//! channels. So the group here is a group of groups - `uart_dma_rx`, from the
//! DMA UART crate, and this crate's own decoder - wired together where both
//! are known, and a firmware selects all four tasks in one declaration.
//!
//! What the channels are for is the application's: `decode` hands each good
//! frame on as [`Channels`], and a firmware binds that call to its own task.

#![no_std]

use ferroforge_task_stm32f4_uart_dma::Port;

/// The sixteen proportional channels of one frame, in SBUS's own 11-bit units,
/// with the receiver's two status flags and running counts.
#[derive(Clone, Copy)]
pub struct Channels {
    pub values: [u16; 16],
    /// The receiver has lost the transmitter and is sending its failsafe.
    pub failsafe: bool,
    /// The receiver missed this frame.
    pub lost: bool,
    /// Frames decoded, and frames refused, since start.
    pub good: u32,
    pub bad: u32,
}

/// The receive half of the DMA UART with an SBUS decoder behind it.
///
/// `decoded = decode` wires the UART's delivery to this crate's decoder, so a
/// firmware selecting the group binds only what the application owns: the
/// port, the USART, and where decoded channels go.
#[ferroforge::group(spawn = [decoded = decode])]
pub mod sbus_rx {
    use super::*;

    pub use ferroforge_task_stm32f4_uart_dma::uart_dma_rx::*;

    /// A frame is 25 bytes: `0x0F`, 22 bytes holding 16 channels of 11 bits
    /// little-endian across byte boundaries, a flags byte, then `0x00`.
    #[ferroforge::task(
        shared = [port: Port],
        local = [good: u32 = 0, bad: u32 = 0],
        spawn = [channels(update: Channels)],
    )]
    pub async fn decode(mut cx: decode::Context, _bytes: usize) {
        let (frame, len) = cx.shared.port.lock(|port| (port.frame, port.frame_len));
        if len < 25 || frame[0] != 0x0F || frame[24] != 0x00 {
            // Counted rather than printed: a misaligned stream would otherwise
            // produce a line per frame, at SBUS's ~140 Hz.
            *cx.local.bad = cx.local.bad.wrapping_add(1);
            if *cx.local.bad % 100 == 1 {
                defmt::warn!(
                    "sbus: {=u32} unparsable frames, last was {=usize} bytes \
                     starting {=u8:#04x}",
                    *cx.local.bad,
                    len,
                    frame[0]
                );
            }
            return;
        }

        let mut values = [0u16; 16];
        let mut bits = 0u32;
        let mut held = 0u32;
        let mut channel = 0usize;
        for byte in &frame[1..23] {
            bits |= u32::from(*byte) << held;
            held += 8;
            while held >= 11 && channel < 16 {
                values[channel] = (bits & 0x07FF) as u16;
                bits >>= 11;
                held -= 11;
                channel += 1;
            }
        }
        *cx.local.good = cx.local.good.wrapping_add(1);

        let update = Channels {
            values,
            failsafe: frame[23] & 0x08 != 0,
            lost: frame[23] & 0x04 != 0,
            good: *cx.local.good,
            bad: *cx.local.bad,
        };
        let _: Result<(), Channels> = cx.spawn.channels(update);
    }
}

/// Both directions of the DMA UART, receiving SBUS: a group of a group of
/// groups and a task, across two crates.
#[ferroforge::group]
pub mod sbus_link {
    pub use super::sbus_rx::*;
    pub use ferroforge_task_stm32f4_uart_dma::on_tx;
}
