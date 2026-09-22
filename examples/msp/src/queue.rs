//! A byte queue that only ever holds whole frames.
//!
//! The distinction matters more than it looks. A transmitter's parser has no
//! way to tell a truncated frame from the start of a longer one, so half a
//! frame does not cost one frame - it costs that frame and the next, and on a
//! link that stays busy it can cost every frame after it. Refusing a frame that
//! does not fit, and saying so, keeps a full queue to one lost update.

/// A fixed-capacity queue of outgoing bytes.
pub struct Frames<const N: usize> {
    bytes: [u8; N],
    head: usize,
    len: usize,
}

impl<const N: usize> Frames<N> {
    pub const fn new() -> Self {
        Self {
            bytes: [0; N],
            head: 0,
            len: 0,
        }
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub const fn capacity(&self) -> usize {
        N
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }

    /// Appends a whole frame, or nothing at all. `false` means the frame was
    /// refused and the queue is unchanged - never that part of it went.
    ///
    /// A frame longer than the whole queue is refused for the same reason,
    /// rather than being wrapped around on top of itself.
    pub fn push(&mut self, frame: &[u8]) -> bool {
        if frame.len() > N - self.len {
            return false;
        }
        for byte in frame {
            let index = (self.head + self.len) % N;
            self.bytes[index] = *byte;
            self.len += 1;
        }
        true
    }

    /// The next byte to put on the wire.
    pub fn pop(&mut self) -> Option<u8> {
        if self.len == 0 {
            return None;
        }
        let byte = self.bytes[self.head];
        self.head = (self.head + 1) % N;
        self.len -= 1;
        Some(byte)
    }
}

impl<const N: usize> Default for Frames<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain<const N: usize>(queue: &mut Frames<N>) -> ([u8; 64], usize) {
        let mut out = [0u8; 64];
        let mut len = 0;
        while let Some(byte) = queue.pop() {
            out[len] = byte;
            len += 1;
        }
        (out, len)
    }

    #[test]
    fn bytes_come_back_in_the_order_they_went_in() {
        let mut queue = Frames::<16>::new();
        assert!(queue.push(&[1, 2, 3]));
        assert!(queue.push(&[4, 5]));
        let (bytes, len) = drain(&mut queue);
        assert_eq!(&bytes[..len], &[1, 2, 3, 4, 5]);
        assert!(queue.is_empty());
    }

    /// The property the whole type exists for.
    #[test]
    fn a_frame_that_does_not_fit_is_refused_whole() {
        let mut queue = Frames::<8>::new();
        assert!(queue.push(&[1, 2, 3, 4, 5]));
        assert!(!queue.push(&[6, 7, 8, 9]));
        assert_eq!(queue.len(), 5);
        let (bytes, len) = drain(&mut queue);
        assert_eq!(&bytes[..len], &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn a_frame_larger_than_the_queue_is_refused_rather_than_wrapped() {
        let mut queue = Frames::<4>::new();
        assert!(!queue.push(&[1, 2, 3, 4, 5]));
        assert!(queue.is_empty());
    }

    #[test]
    fn a_frame_filling_the_queue_exactly_is_accepted() {
        let mut queue = Frames::<4>::new();
        assert!(queue.push(&[1, 2, 3, 4]));
        assert_eq!(queue.len(), 4);
        assert!(!queue.push(&[5]));
    }

    /// Draining and refilling has to reuse the space, or a long-running link
    /// stops accepting frames after the first lap.
    #[test]
    fn the_queue_keeps_working_past_its_own_length() {
        let mut queue = Frames::<8>::new();
        let mut expected = 0u8;
        let mut seen = 0u8;
        for _ in 0..50 {
            assert!(queue.push(&[expected, expected.wrapping_add(1), expected.wrapping_add(2)]));
            expected = expected.wrapping_add(3);
            for _ in 0..3 {
                assert_eq!(queue.pop(), Some(seen));
                seen = seen.wrapping_add(1);
            }
        }
        assert!(queue.is_empty());
    }

    /// Writes that straddle the wrap are the case an index bug hides in.
    #[test]
    fn a_frame_straddling_the_wrap_comes_back_intact() {
        let mut queue = Frames::<8>::new();
        assert!(queue.push(&[0; 6]));
        for _ in 0..6 {
            queue.pop();
        }
        assert!(queue.push(&[1, 2, 3, 4, 5]));
        let (bytes, len) = drain(&mut queue);
        assert_eq!(&bytes[..len], &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn an_empty_queue_has_nothing_to_send() {
        let mut queue = Frames::<8>::new();
        assert_eq!(queue.pop(), None);
        assert!(queue.is_empty());
        assert_eq!(queue.capacity(), 8);
    }

    #[test]
    fn clearing_discards_everything_pending() {
        let mut queue = Frames::<8>::new();
        queue.push(&[1, 2, 3]);
        queue.clear();
        assert!(queue.is_empty());
        assert_eq!(queue.pop(), None);
        assert!(queue.push(&[4, 5, 6, 7, 8, 9, 10, 11]));
    }
}
