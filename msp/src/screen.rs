//! The text canvas a DisplayPort VTX draws.
//!
//! Sized for the smallest canvas in circulation rather than the largest: an
//! analogue-resolution DJI or HDZero canvas is 30x16, and an HD one is bigger.
//! Text that fits the small one shows on every goggle; text laid out for the
//! big one is clipped on some and not others, which is worse than being plain.

pub const ROWS: usize = 16;
pub const COLS: usize = 30;

/// What the goggles should show, as characters.
///
/// Whole rows rather than dirty cells: a DisplayPort write carries a row, a
/// column and a run of characters, so a row is the unit the wire already has.
pub struct Screen {
    cells: [[u8; COLS]; ROWS],
}

impl Screen {
    pub const fn new() -> Self {
        Self {
            cells: [[b' '; COLS]; ROWS],
        }
    }

    pub fn clear(&mut self) {
        self.cells = [[b' '; COLS]; ROWS];
    }

    /// Writes `text` at `row`, `column`, clipped to the canvas. Out of range is
    /// a no-op rather than a panic: a screen is cosmetic, and taking a firmware
    /// down over a mis-placed label would not be.
    pub fn write(&mut self, row: usize, column: usize, text: &[u8]) {
        if row >= ROWS || column >= COLS {
            return;
        }
        let room = COLS - column;
        let len = text.len().min(room);
        self.cells[row][column..column + len].copy_from_slice(&text[..len]);
    }

    /// Replaces a row outright, so a shorter value cannot leave the tail of a
    /// longer one behind it.
    pub fn set_row(&mut self, row: usize, text: &[u8]) {
        if row >= ROWS {
            return;
        }
        self.cells[row] = [b' '; COLS];
        let len = text.len().min(COLS);
        self.cells[row][..len].copy_from_slice(&text[..len]);
    }

    /// A row with its trailing spaces removed, which is what gets transmitted.
    /// An all-space row comes back empty, and the caller skips it.
    pub fn row(&self, row: usize) -> &[u8] {
        if row >= ROWS {
            return &[];
        }
        let cells = &self.cells[row];
        let mut len = COLS;
        while len > 0 && cells[len - 1] == b' ' {
            len -= 1;
        }
        &cells[..len]
    }
}

impl Default for Screen {
    fn default() -> Self {
        Self::new()
    }
}

/// One row under construction.
///
/// Built in place rather than formatted: `core::fmt` pulls in machinery a
/// firmware pays for in flash, and every field here is a label and a number.
pub struct Line {
    bytes: [u8; COLS],
    len: usize,
}

impl Line {
    pub const fn new() -> Self {
        Self {
            bytes: [b' '; COLS],
            len: 0,
        }
    }

    /// Appends literal text, silently stopping at the edge of the row.
    pub fn text(&mut self, text: &[u8]) -> &mut Self {
        let room = COLS - self.len;
        let len = text.len().min(room);
        self.bytes[self.len..self.len + len].copy_from_slice(&text[..len]);
        self.len += len;
        self
    }

    /// Appends a decimal number right-aligned in `width` columns, so a column
    /// of them lines up and a value changing from 999 to 1000 does not shift
    /// everything after it.
    pub fn number(&mut self, value: u16, width: usize) -> &mut Self {
        let mut digits = [0u8; 5];
        let mut remaining = value;
        let mut count = 0;
        loop {
            digits[count] = b'0' + (remaining % 10) as u8;
            count += 1;
            remaining /= 10;
            if remaining == 0 {
                break;
            }
        }
        for _ in count..width {
            self.text(b" ");
        }
        for index in (0..count).rev() {
            let digit = digits[index];
            self.text(&[digit]);
        }
        self
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

impl Default for Line {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_screen_has_no_rows_to_send() {
        let screen = Screen::new();
        for row in 0..ROWS {
            assert_eq!(screen.row(row), b"");
        }
    }

    #[test]
    fn a_row_comes_back_without_its_padding() {
        let mut screen = Screen::new();
        screen.write(2, 4, b"CH0");
        assert_eq!(screen.row(2), b"    CH0");
    }

    /// The reason `set_row` exists: 1000 then 985 must not read "9850".
    #[test]
    fn setting_a_row_erases_what_was_longer() {
        let mut screen = Screen::new();
        screen.set_row(3, b"CH0 1000");
        screen.set_row(3, b"CH0 985");
        assert_eq!(screen.row(3), b"CH0 985");
    }

    #[test]
    fn writing_past_the_edge_clips_instead_of_panicking() {
        let mut screen = Screen::new();
        screen.write(0, COLS - 3, b"ABCDEFG");
        assert_eq!(screen.row(0).len(), COLS);
        screen.write(ROWS, 0, b"nowhere");
        screen.write(0, COLS, b"nowhere");
        assert_eq!(screen.row(ROWS), b"");
    }

    #[test]
    fn numbers_are_right_aligned_so_a_column_of_them_lines_up() {
        let mut line = Line::new();
        line.text(b"CH0").number(985, 5);
        assert_eq!(line.as_bytes(), b"CH0  985");

        let mut wide = Line::new();
        wide.text(b"CH0").number(1811, 5);
        assert_eq!(wide.as_bytes(), b"CH0 1811");
    }

    #[test]
    fn a_number_wider_than_its_field_is_not_truncated() {
        let mut line = Line::new();
        line.number(12345, 2);
        assert_eq!(line.as_bytes(), b"12345");
    }

    #[test]
    fn zero_is_a_digit() {
        let mut line = Line::new();
        line.number(0, 3);
        assert_eq!(line.as_bytes(), b"  0");
    }

    #[test]
    fn a_line_stops_at_the_edge_of_the_row() {
        let mut line = Line::new();
        for _ in 0..COLS {
            line.text(b"ab");
        }
        assert_eq!(line.as_bytes().len(), COLS);
    }
}
