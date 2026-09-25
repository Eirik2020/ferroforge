//! EXPERIMENT: one-writer, one-reader mailbox between the H755's two cores,
//! at an SRAM4 address both linker scripts define.
//!
//! It is an `extern` symbol rather than a `static` on purpose. As a static,
//! fat LTO saw that the M7 image never stores to it, folded every read to the
//! initial value and dropped the object; the same happened to embassy's
//! `SharedData`, whose stores were never loaded in the M7 image. Memory the
//! other core writes has to be invisible to this image's optimizer.
//!
//! A sequence lock: the M4 writes, the M7 reads without ever blocking, and a
//! torn read is detected and retried rather than locked out. SRAM4 must stay
//! non-cacheable on the M7 (its D-cache is off here) for this to hold.

#![no_std]

use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU32, Ordering, fence};

unsafe extern "C" {
    // Defined in both images' memory.x at the same SRAM4 addresses.
    static __spike_mailbox: Mailbox;
    static __spike_embassy_shared_data: [u8; 0];
}

/// The mailbox both images see at one address.
pub fn mailbox() -> &'static Mailbox {
    // SAFETY: the linker places the symbol in SRAM4, reserved for it in both
    // images; every field is an atomic, so any bit pattern is a valid value.
    unsafe { &__spike_mailbox }
}

/// Embassy's dual-core `SharedData`, at the address both images reserve.
pub fn embassy_shared<T>() -> &'static MaybeUninit<T> {
    // SAFETY: `MaybeUninit` makes no validity claim; the region is reserved
    // for this in both linker scripts and is large enough for `SharedData`.
    unsafe { &*(&raw const __spike_embassy_shared_data).cast::<MaybeUninit<T>>() }
}

/// Written by the M4 (receiver side), read by the M7 (flight side).
#[repr(C)]
pub struct Mailbox {
    sequence: AtomicU32,
    channels: [AtomicU32; 8],
    received_at_us: AtomicU32,
}

impl Mailbox {
    pub const fn new() -> Self {
        Self {
            sequence: AtomicU32::new(0),
            channels: [const { AtomicU32::new(0) }; 8],
            received_at_us: AtomicU32::new(0),
        }
    }

    /// M4 only, before the first `publish`: SRAM4 is not cleared at reset.
    pub fn reset(&self) {
        self.sequence.store(0, Ordering::Release);
    }

    /// M4 only. Odd sequence marks a write in progress.
    pub fn publish(&self, channels: &[u16; 8], received_at_us: u32) {
        let sequence = self.sequence.load(Ordering::Relaxed);
        self.sequence.store(sequence.wrapping_add(1), Ordering::Relaxed);
        fence(Ordering::Release);
        for (slot, value) in self.channels.iter().zip(channels) {
            slot.store(*value as u32, Ordering::Relaxed);
        }
        self.received_at_us.store(received_at_us, Ordering::Relaxed);
        fence(Ordering::Release);
        self.sequence.store(sequence.wrapping_add(2), Ordering::Relaxed);
    }

    /// M7 only. `None` while the M4 is mid-write; the caller keeps its last
    /// frame and its own staleness check decides what that means.
    pub fn read(&self) -> Option<(u32, [u16; 8], u32)> {
        let before = self.sequence.load(Ordering::Acquire);
        if before & 1 == 1 {
            return None;
        }
        let mut channels = [0u16; 8];
        for (value, slot) in channels.iter_mut().zip(&self.channels) {
            *value = slot.load(Ordering::Relaxed) as u16;
        }
        let received_at_us = self.received_at_us.load(Ordering::Relaxed);
        fence(Ordering::Acquire);
        (self.sequence.load(Ordering::Relaxed) == before).then_some((before, channels, received_at_us))
    }
}
