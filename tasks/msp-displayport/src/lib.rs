//! An MSP DisplayPort OSD: the task that keeps a pilot's goggles drawing.
//!
//! A video transmitter running DisplayPort is not a screen the firmware writes
//! to. It is a peer. It has to be answered before it will hand over the canvas,
//! and it takes the canvas back if the conversation stops - so an OSD is a
//! picture pushed out on a timer and a conversation kept up underneath it.
//!
//! Nothing here names a UART, which is the whole reason this crate is portable.
//! [`paint`] fills a queue of whole frames and says how much is waiting; a
//! firmware's own hardware task owns the port, puts those bytes on the wire,
//! and hands received bytes back through [`Link::feed`]. Which serial port this
//! is cannot be a property of an OSD.
//!
//! Receiving is deliberately not a task. The handler that drains the queue has
//! to hold [`Link`] anyway, and a byte-at-a-time parser step costs less than a
//! second lock would - never mind a task spawn per byte, which would drop bytes
//! whenever the previous one had not finished.
//!
//! The protocol itself is [`msp`], which has no dependencies and carries the
//! host tests. What is left here is the part that only means anything inside an
//! RTIC application.

#![no_std]

/// Re-exported so a firmware can build screen rows without naming the
/// protocol crate in its own manifest.
pub use msp;

use fugit::ExtU32 as _;
use msp::{Parser, Screen, Telemetry, codec::MAX_FRAME, displayport, poll, queue::Frames};

/// Room for the outgoing frames of one refresh, with margin.
///
/// A refresh is a clear, one write per occupied row and a draw, so the worst
/// case is set by how much text a screen carries rather than by anything here.
/// At 115200 baud a full queue takes about 89 ms to drain, which is the real
/// constraint on how fast [`paint`] can usefully be asked to run.
pub const TX_CAPACITY: usize = 1024;

/// The whole conversation with one transmitter.
///
/// One type rather than a screen, a parser and a queue bound separately: a
/// firmware that wired those to different resources would compile, and would
/// then answer the transmitter out of one conversation while drawing from
/// another.
pub struct Link {
    /// What the goggles should show. The application writes here; [`paint`]
    /// reads it and nothing else does.
    pub screen: Screen,
    /// What the transmitter is told when it asks. Written by the application
    /// for the same reason.
    pub telemetry: Telemetry,
    parser: Parser,
    outgoing: Frames<TX_CAPACITY>,
    /// Completed screen refreshes.
    pub refreshes: u32,
    /// Requests answered. This is the number that says whether the transmitter
    /// is talking to us at all, so it is worth a log line.
    pub answered: u32,
    /// Frames dropped because the queue was full. Whole frames: see
    /// [`msp::queue`] for why a partial one is never an option.
    pub refused: u32,
    /// Malformed bytes from the transmitter - a bad checksum, or a length this
    /// end cannot hold.
    pub rejected: u32,
}

impl Link {
    pub const fn new() -> Self {
        Self {
            screen: Screen::new(),
            telemetry: Telemetry::new(),
            parser: Parser::new(),
            outgoing: Frames::new(),
            refreshes: 0,
            answered: 0,
            refused: 0,
            rejected: 0,
        }
    }

    /// Queues one whole picture: clear, every occupied row, draw, heartbeat.
    ///
    /// Clearing every time rather than tracking which rows changed is what
    /// makes skipping the blank rows safe - a row that empties is erased by the
    /// clear rather than left standing. The heartbeat is not decoration: without
    /// one the transmitter takes the canvas back and shows its own OSD.
    pub fn refresh(&mut self) {
        let mut frame = [0u8; MAX_FRAME];

        if let Ok(len) = displayport::clear_screen(&mut frame) {
            self.queue(&frame[..len]);
        }

        for row in 0..msp::ROWS {
            let text = self.screen.row(row);
            if text.is_empty() {
                continue;
            }
            let built = displayport::write_string(row as u8, 0, 0, text, &mut frame);
            if let Ok(len) = built {
                self.queue(&frame[..len]);
            }
        }

        if let Ok(len) = displayport::draw_screen(&mut frame) {
            self.queue(&frame[..len]);
        }
        if let Ok(len) = displayport::heartbeat(&mut frame) {
            self.queue(&frame[..len]);
        }

        self.refreshes = self.refreshes.wrapping_add(1);
    }

    /// Takes one received byte and queues a reply if that byte completed a
    /// request. Cheap enough to call from the interrupt that received it.
    pub fn feed(&mut self, byte: u8) {
        match self.parser.push(byte) {
            Ok(Some(request)) => {
                let mut frame = [0u8; MAX_FRAME];
                if let Some(len) = poll::respond(&request, &self.telemetry, &mut frame) {
                    self.answered = self.answered.wrapping_add(1);
                    self.queue(&frame[..len]);
                }
            }
            Ok(None) => {}
            Err(_) => self.rejected = self.rejected.wrapping_add(1),
        }
    }

    /// The next byte for the wire, for whichever hardware task owns the port.
    pub fn next_byte(&mut self) -> Option<u8> {
        self.outgoing.pop()
    }

    pub fn pending(&self) -> usize {
        self.outgoing.len()
    }

    fn queue(&mut self, frame: &[u8]) {
        if !self.outgoing.push(frame) {
            self.refused = self.refused.wrapping_add(1);
        }
    }
}

impl Default for Link {
    fn default() -> Self {
        Self::new()
    }
}

/// Pushes the picture out on a timer.
///
/// Async and self-scheduling, so a firmware needs no timer task of its own to
/// drive it. `refresh_ms` is the whole cadence: too slow and the transmitter
/// reclaims the canvas, too fast and the queue outruns the baud rate. Around
/// ten times a second suits both.
///
/// `kick` carries how many bytes are waiting, and exists because the library
/// cannot start a transmission itself. A firmware binds it to whatever makes
/// its port ask for the first byte.
#[ferroforge::task(
    shared = [link: Link],
    config = [refresh_ms: u32],
    spawn = [kick(pending: usize)],
    monotonic = Mono,
)]
pub async fn paint(mut cx: paint::Context) -> ! {
    loop {
        let pending = cx.shared.link.lock(|link| {
            link.refresh();
            link.pending()
        });
        if pending > 0 {
            let _: Result<(), usize> = cx.spawn.kick(pending);
        }
        Mono::delay(CONFIG::REFRESH_MS.millis()).await;
    }
}
