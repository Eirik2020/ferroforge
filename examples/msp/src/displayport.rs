//! The DisplayPort subcommands, all carried inside MSP command 182.
//!
//! A flight controller does not draw anything itself here: it ships rows of
//! characters to the video transmitter, which renders them into the pilot's
//! goggles. A refresh is therefore a short conversation - clear, some rows,
//! draw - rather than a frame buffer.

use crate::codec::{Direction, Error, MAX_FRAME, encode};

/// `MSP_DISPLAYPORT`. Every subcommand below travels as this command's first
/// payload byte.
pub const COMMAND: u8 = 182;

pub const HEARTBEAT: u8 = 0;
pub const RELEASE: u8 = 1;
pub const CLEAR_SCREEN: u8 = 2;
pub const WRITE_STRING: u8 = 3;
pub const DRAW_SCREEN: u8 = 4;
pub const OPTIONS: u8 = 5;

/// The most characters one write carries. The payload also holds the
/// subcommand, the row, the column and the attribute byte.
pub const MAX_TEXT: usize = 30;

fn subcommand(which: u8, output: &mut [u8; MAX_FRAME]) -> Result<usize, Error> {
    encode(Direction::FromFlightController, COMMAND, &[which], output)
}

/// Tells the transmitter to keep the MSP canvas up. Without it the VTX falls
/// back to its own OSD after a second or two, so this is not optional decoration.
pub fn heartbeat(output: &mut [u8; MAX_FRAME]) -> Result<usize, Error> {
    subcommand(HEARTBEAT, output)
}

/// Hands the canvas back, so the transmitter draws its own OSD again.
pub fn release(output: &mut [u8; MAX_FRAME]) -> Result<usize, Error> {
    subcommand(RELEASE, output)
}

pub fn clear_screen(output: &mut [u8; MAX_FRAME]) -> Result<usize, Error> {
    subcommand(CLEAR_SCREEN, output)
}

/// Commits everything written since the last clear. Rows written without this
/// never reach the pilot.
pub fn draw_screen(output: &mut [u8; MAX_FRAME]) -> Result<usize, Error> {
    subcommand(DRAW_SCREEN, output)
}

/// Places a run of characters. `attribute` is zero for ordinary text.
pub fn write_string(
    row: u8,
    column: u8,
    attribute: u8,
    text: &[u8],
    output: &mut [u8; MAX_FRAME],
) -> Result<usize, Error> {
    let len = text.len().min(MAX_TEXT);
    let mut payload = [0u8; MAX_TEXT + 4];
    payload[0] = WRITE_STRING;
    payload[1] = row;
    payload[2] = column;
    payload[3] = attribute;
    payload[4..4 + len].copy_from_slice(&text[..len]);
    encode(
        Direction::FromFlightController,
        COMMAND,
        &payload[..4 + len],
        output,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{MAX_FRAME, Parser};

    fn parsed(bytes: &[u8]) -> (u8, [u8; 64], usize) {
        let mut parser = Parser::new();
        let mut result = None;
        for byte in bytes {
            result = parser.push(*byte).unwrap().or(result);
        }
        let packet = result.expect("a complete frame");
        let mut payload = [0u8; 64];
        let len = packet.payload().len();
        payload[..len].copy_from_slice(packet.payload());
        (packet.command, payload, len)
    }

    #[test]
    fn a_write_carries_its_position_ahead_of_its_text() {
        let mut output = [0; MAX_FRAME];
        let len = write_string(3, 2, 0, b"CH0  985", &mut output).unwrap();
        let (command, payload, payload_len) = parsed(&output[..len]);
        assert_eq!(command, COMMAND);
        assert_eq!(&payload[..4], &[WRITE_STRING, 3, 2, 0]);
        assert_eq!(&payload[4..payload_len], b"CH0  985");
    }

    #[test]
    fn the_bare_subcommands_carry_nothing_else() {
        for (build, expected) in [
            (clear_screen as fn(&mut [u8; MAX_FRAME]) -> _, CLEAR_SCREEN),
            (draw_screen, DRAW_SCREEN),
            (heartbeat, HEARTBEAT),
            (release, RELEASE),
        ] {
            let mut output = [0; MAX_FRAME];
            let len = build(&mut output).unwrap();
            let (command, payload, payload_len) = parsed(&output[..len]);
            assert_eq!(command, COMMAND);
            assert_eq!(payload_len, 1);
            assert_eq!(payload[0], expected);
        }
    }

    /// A row longer than the canvas must not push the payload past the frame.
    #[test]
    fn an_overlong_row_is_clipped_rather_than_refused() {
        let mut output = [0; MAX_FRAME];
        let text = [b'X'; MAX_TEXT * 2];
        let len = write_string(0, 0, 0, &text, &mut output).unwrap();
        let (_, _, payload_len) = parsed(&output[..len]);
        assert_eq!(payload_len, MAX_TEXT + 4);
    }
}
