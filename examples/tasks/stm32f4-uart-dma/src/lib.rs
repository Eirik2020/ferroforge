//! A DMA UART receiver: several RTIC tasks that only work as a set.
//!
//! This is the case a single task definition cannot express. Bytes arrive by
//! DMA into a circular buffer that no interrupt ever stops, so "how much has
//! arrived" is the controller's remaining-transfer count, not an event. Two
//! interrupts to watch: the line going idle, which is the only frame boundary a
//! protocol without a length field has, and the stream wrapping, which is not a
//! boundary at all but is the only warning that unread bytes are about to be
//! overwritten. A third task does the work off the interrupt.
//!
//! Choosing priorities is the firmware's business, as in any RTIC application,
//! and nothing here checks them. What works: `on_uart` and `on_rx` both lock
//! [`Port`] from interrupt context, so at one priority RTIC's lock is free and
//! at two it puts a critical section inside both handlers. `parse` exists to
//! run after the receive interrupts rather than instead of them, so it belongs
//! below both.
//!
//! Framing is left to the application: this delivers bytes and says how many,
//! and an ordinary RTIC task decodes them.

#![no_std]

use stm32f4xx_hal::{
    dma::{Stream2, Stream7, traits::Stream, traits::StreamISR},
    pac::{DMA2, USART1},
};

/// How much the DMA controller writes into before wrapping. Larger than any one
/// frame, so a frame is never split across the wrap more than once.
pub const RING: usize = 64;

/// The most a single `frame` delivery will report.
pub const MAX_FRAME: usize = 32;

/// Everything these tasks share.
///
/// One type rather than a buffer, a cursor and a counter bound separately: a
/// firmware that wired those to different resources would compile and corrupt.
pub struct Port {
    /// The stream lives here because reading its remaining-transfer count *is*
    /// reading the write cursor: two tasks need it, and an RTIC local can only
    /// belong to one. Holding it beside the buffer it fills also means a
    /// firmware cannot wire the cursor and the ring to different places.
    pub stream: Stream2<DMA2>,
    /// Written by the DMA controller, never by software.
    ///
    /// Borrowed rather than owned: the controller needs an address that never
    /// moves, and where that memory lives is the firmware's decision rather
    /// than a `static` hidden in a library. The firmware owns the buffer.
    ring: &'static mut [u8; RING],
    /// How far software has consumed. The write position is not stored - it is
    /// read back from the controller, because only it knows.
    pub tail: usize,
    /// The most recent frame, copied out of the ring so a decoder can read it
    /// without holding the lock.
    pub frame: [u8; MAX_FRAME],
    pub frame_len: usize,
    pub frames: u32,
    /// Deliveries larger than `MAX_FRAME`, so software fell behind the ring.
    pub overruns: u32,
    /// Hardware receive errors: overrun, noise, framing, parity. An overrun in
    /// particular stops the USART requesting DMA until it is cleared, so these
    /// are not merely informational.
    pub errors: u32,
}

impl Port {
    /// Takes the stream and buffer already configured and started by `init`:
    /// setting up a transfer is native HAL work and belongs in the firmware,
    /// per G6. `ring` must be the buffer the stream was pointed at.
    pub const fn new(stream: Stream2<DMA2>, ring: &'static mut [u8; RING]) -> Self {
        Self {
            stream,
            ring,
            tail: 0,
            frame: [0; MAX_FRAME],
            frame_len: 0,
            frames: 0,
            overruns: 0,
            errors: 0,
        }
    }

    /// Where the controller has written to, derived from how much of the
    /// circular transfer is left. Only it knows, so nothing caches this.
    pub fn head(&self) -> usize {
        RING - (self.stream.number_of_transfers() as usize % RING)
    }

    /// Copy everything between `tail` and `head` into `frame`, wrapping as
    /// needed, and advance `tail`. Returns how much was taken.
    fn take(&mut self, head: usize) -> usize {
        let available = head.wrapping_sub(self.tail) % RING;
        if available == 0 {
            return 0;
        }
        if available > MAX_FRAME {
            self.overruns = self.overruns.wrapping_add(1);
        }
        let taken = available.min(MAX_FRAME);
        for index in 0..taken {
            self.frame[index] = self.ring[(self.tail + index) % RING];
        }
        self.tail = (self.tail + taken) % RING;
        self.frame_len = taken;
        self.frames = self.frames.wrapping_add(1);
        taken
    }
}

/// The USART's own interrupt. An idle line is the end of a frame on a protocol
/// with no length field, which is most of them.
#[ferroforge::task(
    shared = [port: Port],
    local = [uart: USART1],
    spawn = [frame(bytes: usize)],
)]
pub fn on_uart(mut cx: on_uart::Context) {
    let status = cx.local.uart.sr().read();

    // An overrun is not informational: while ORE is set the USART stops setting
    // RXNE, so it stops requesting DMA, and reception is wedged until this runs.
    // Clearing it costs the byte that was lost anyway.
    let failed = status.ore().bit_is_set()
        || status.nf().bit_is_set()
        || status.fe().bit_is_set()
        || status.pe().bit_is_set();

    let idle = status.idle().bit_is_set();
    if !failed && !idle {
        return;
    }

    // On an F4 there is no flag-clear register: reading SR then DR is what
    // clears IDLE and every error flag, and both reads are required. At IDLE the
    // line is quiet by definition, so this does not race a byte being received.
    let _ = cx.local.uart.dr().read();

    let taken = cx.shared.port.lock(|port| {
        if failed {
            port.errors = port.errors.wrapping_add(1);
        }
        // Whatever arrived before the error is still worth delivering.
        let head = port.head();
        port.take(head)
    });
    if taken > 0 {
        let _: Result<(), usize> = cx.spawn.frame(taken);
    }
}

/// The receive stream wrapped.
///
/// This deliberately delivers nothing. In circular mode a transfer-complete is
/// not a frame boundary - it is simply the buffer filling - so delivering here
/// cuts whatever frame happened to be in flight into two short pieces. On a
/// protocol with no length field the idle line is the only boundary there is.
///
/// What it is good for is noticing that the reader is about to be lapped, which
/// is the one thing a circular transfer will not tell you by stopping.
#[ferroforge::task(shared = [port: Port])]
pub fn on_rx(mut cx: on_rx::Context) {
    cx.shared.port.lock(|port| {
        if !port.stream.is_transfer_complete() {
            // Something else sharing this interrupt raised it.
            return;
        }
        // Not optional: an uncleared flag re-enters the handler on return.
        port.stream.clear_transfer_complete();

        // Unread bytes this close to a full lap are about to be overwritten.
        let head = port.head();
        if head.wrapping_sub(port.tail) % RING > RING - MAX_FRAME {
            port.overruns = port.overruns.wrapping_add(1);
        }
    });
}

/// The transmit stream drained. Nothing here touches `Port`, so its priority
/// has no bearing on the others'.
#[ferroforge::task(local = [stream: Stream7<DMA2>, sent: u32])]
pub fn on_tx(cx: on_tx::Context) {
    if !cx.local.stream.is_transfer_complete() {
        return;
    }
    cx.local.stream.clear_transfer_complete();
    *cx.local.sent = cx.local.sent.wrapping_add(1);
}

/// Off the interrupt, and the only task allowed to be slow. It hands the frame
/// on rather than interpreting it: what the bytes mean is the application's.
#[ferroforge::task(spawn = [decoded(bytes: usize)])]
pub async fn parse(cx: parse::Context, bytes: usize) {
    let _: Result<(), usize> = cx.spawn.decoded(bytes);
}
