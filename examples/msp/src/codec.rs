//! MSP v1 framing: `$M<dir><len><cmd><payload><checksum>`.
//!
//! The checksum is an XOR fold over everything from the length byte onwards,
//! which is why the encoder can compute it in one pass over the buffer it just
//! wrote.

/// The largest payload either direction carries here. A DisplayPort write is a
/// subcommand, a row, a column, an attribute and up to thirty characters, so
/// this has room to spare.
pub const MAX_PAYLOAD: usize = 64;

/// Payload plus `$M`, direction, length, command and checksum.
pub const MAX_FRAME: usize = MAX_PAYLOAD + 6;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The output buffer is shorter than the frame.
    OutputTooSmall,
    /// More than [`MAX_PAYLOAD`] bytes.
    PayloadTooLong,
    /// The trailing byte disagreed with the running XOR.
    Checksum { expected: u8, calculated: u8 },
}

/// Who a frame is addressed to. The VTX sends `<` and expects `>` back.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Direction {
    ToFlightController,
    FromFlightController,
    Failed,
}

impl Direction {
    pub const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            b'<' => Some(Self::ToFlightController),
            b'>' => Some(Self::FromFlightController),
            b'!' => Some(Self::Failed),
            _ => None,
        }
    }

    pub const fn as_byte(self) -> u8 {
        match self {
            Self::ToFlightController => b'<',
            Self::FromFlightController => b'>',
            Self::Failed => b'!',
        }
    }
}

/// One decoded request.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Packet {
    pub command: u8,
    pub direction: Direction,
    length: usize,
    bytes: [u8; MAX_PAYLOAD],
}

impl Packet {
    pub fn payload(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum State {
    Dollar,
    M,
    Direction,
    Length,
    Command,
    Payload,
    Checksum,
}

/// Feeds on one byte at a time, because that is how they arrive.
pub struct Parser {
    state: State,
    direction: Direction,
    command: u8,
    length: usize,
    position: usize,
    bytes: [u8; MAX_PAYLOAD],
    checksum: u8,
}

impl Parser {
    pub const fn new() -> Self {
        Self {
            state: State::Dollar,
            direction: Direction::ToFlightController,
            command: 0,
            length: 0,
            position: 0,
            bytes: [0; MAX_PAYLOAD],
            checksum: 0,
        }
    }

    /// `Ok(Some(_))` on the checksum byte of a good frame. An error resets the
    /// parser rather than wedging it: the next `$` starts a new frame, so one
    /// corrupted frame costs one frame.
    pub fn push(&mut self, byte: u8) -> Result<Option<Packet>, Error> {
        match self.state {
            State::Dollar => {
                if byte == b'$' {
                    self.state = State::M;
                }
            }
            State::M => {
                if byte == b'M' {
                    self.state = State::Direction;
                } else {
                    self.reset();
                }
            }
            State::Direction => match Direction::from_byte(byte) {
                Some(direction) => {
                    self.direction = direction;
                    self.state = State::Length;
                }
                None => self.reset(),
            },
            State::Length => {
                let length = byte as usize;
                if length > MAX_PAYLOAD {
                    self.reset();
                    return Err(Error::PayloadTooLong);
                }
                self.length = length;
                self.position = 0;
                self.checksum = byte;
                self.state = State::Command;
            }
            State::Command => {
                self.command = byte;
                self.checksum ^= byte;
                self.state = match self.length {
                    0 => State::Checksum,
                    _ => State::Payload,
                };
            }
            State::Payload => {
                self.bytes[self.position] = byte;
                self.position += 1;
                self.checksum ^= byte;
                if self.position == self.length {
                    self.state = State::Checksum;
                }
            }
            State::Checksum => {
                let calculated = self.checksum;
                if byte != calculated {
                    self.reset();
                    return Err(Error::Checksum {
                        expected: byte,
                        calculated,
                    });
                }
                let packet = Packet {
                    command: self.command,
                    direction: self.direction,
                    length: self.length,
                    bytes: self.bytes,
                };
                self.reset();
                return Ok(Some(packet));
            }
        }
        Ok(None)
    }

    fn reset(&mut self) {
        self.state = State::Dollar;
        self.direction = Direction::ToFlightController;
        self.command = 0;
        self.length = 0;
        self.position = 0;
        self.checksum = 0;
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

/// Writes one frame into `output` and returns its length.
pub fn encode(
    direction: Direction,
    command: u8,
    payload: &[u8],
    output: &mut [u8],
) -> Result<usize, Error> {
    if payload.len() > MAX_PAYLOAD {
        return Err(Error::PayloadTooLong);
    }
    let frame_len = payload.len() + 6;
    if output.len() < frame_len {
        return Err(Error::OutputTooSmall);
    }

    output[0] = b'$';
    output[1] = b'M';
    output[2] = direction.as_byte();
    output[3] = payload.len() as u8;
    output[4] = command;
    output[5..5 + payload.len()].copy_from_slice(payload);

    let mut checksum = output[3] ^ output[4];
    for byte in payload {
        checksum ^= *byte;
    }
    output[frame_len - 1] = checksum;
    Ok(frame_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(direction: Direction, command: u8, payload: &[u8]) -> Packet {
        let mut bytes = [0; MAX_FRAME];
        let len = encode(direction, command, payload, &mut bytes).unwrap();
        let mut parser = Parser::new();
        let mut parsed = None;
        for byte in &bytes[..len] {
            parsed = parser.push(*byte).unwrap().or(parsed);
        }
        parsed.unwrap()
    }

    #[test]
    fn a_frame_survives_its_own_encoding() {
        let packet = round_trip(Direction::FromFlightController, 182, &[3, 5, 14, 0, b'A']);
        assert_eq!(packet.direction, Direction::FromFlightController);
        assert_eq!(packet.command, 182);
        assert_eq!(packet.payload(), &[3, 5, 14, 0, b'A']);
    }

    #[test]
    fn an_empty_payload_is_a_frame_too() {
        let packet = round_trip(Direction::ToFlightController, 1, &[]);
        assert_eq!(packet.command, 1);
        assert_eq!(packet.payload(), &[]);
    }

    /// Bytes arrive as the UART delivers them, not as frames.
    #[test]
    fn a_frame_split_across_reads_still_parses() {
        let mut bytes = [0; MAX_FRAME];
        let len = encode(Direction::ToFlightController, 101, &[1, 2, 3, 4], &mut bytes).unwrap();

        let mut parser = Parser::new();
        let mut parsed = None;
        for chunk in bytes[..len].chunks(3) {
            for byte in chunk {
                parsed = parser.push(*byte).unwrap().or(parsed);
            }
        }
        assert_eq!(parsed.unwrap().command, 101);
    }

    #[test]
    fn back_to_back_frames_are_two_packets() {
        let mut first = [0; MAX_FRAME];
        let mut second = [0; MAX_FRAME];
        let first_len = encode(Direction::ToFlightController, 105, &[10], &mut first).unwrap();
        let second_len = encode(Direction::ToFlightController, 110, &[20], &mut second).unwrap();

        let mut parser = Parser::new();
        let mut commands = [0u8; 2];
        let mut count = 0;
        for byte in first[..first_len].iter().chain(second[..second_len].iter()) {
            if let Some(packet) = parser.push(*byte).unwrap() {
                commands[count] = packet.command;
                count += 1;
            }
        }
        assert_eq!(count, 2);
        assert_eq!(commands, [105, 110]);
    }

    /// The property that matters on a noisy link: one bad frame costs one frame.
    #[test]
    fn a_bad_checksum_costs_one_frame_and_no_more() {
        let mut bad = [0; MAX_FRAME];
        let mut good = [0; MAX_FRAME];
        let bad_len = encode(Direction::ToFlightController, 101, &[1], &mut bad).unwrap();
        let good_len = encode(Direction::ToFlightController, 105, &[2], &mut good).unwrap();
        bad[bad_len - 1] ^= 0xff;

        let mut parser = Parser::new();
        let mut saw_checksum_error = false;
        let mut parsed = None;
        for byte in bad[..bad_len].iter().chain(good[..good_len].iter()) {
            match parser.push(*byte) {
                Err(Error::Checksum { .. }) => saw_checksum_error = true,
                Ok(Some(packet)) => parsed = Some(packet),
                Ok(None) => {}
                Err(other) => panic!("unexpected error: {other:?}"),
            }
        }
        assert!(saw_checksum_error);
        assert_eq!(parsed.unwrap().command, 105);
    }

    /// A length byte the buffer cannot hold must be rejected at the length, not
    /// written past the end of the payload array.
    #[test]
    fn an_oversized_length_is_refused_before_any_payload_byte() {
        let mut parser = Parser::new();
        assert_eq!(parser.push(b'$'), Ok(None));
        assert_eq!(parser.push(b'M'), Ok(None));
        assert_eq!(parser.push(b'<'), Ok(None));
        assert_eq!(parser.push(255), Err(Error::PayloadTooLong));

        // And the parser is usable immediately afterwards.
        let packet = round_trip(Direction::ToFlightController, 105, &[7]);
        assert_eq!(packet.command, 105);
    }

    #[test]
    fn garbage_before_a_frame_is_skipped() {
        let mut bytes = [0; MAX_FRAME];
        let len = encode(Direction::ToFlightController, 105, &[9], &mut bytes).unwrap();
        let mut parser = Parser::new();
        let mut parsed = None;
        for byte in b"\x00\xff$$M".iter().chain(bytes[..len].iter()) {
            parsed = parser.push(*byte).unwrap().or(parsed);
        }
        assert_eq!(parsed.unwrap().command, 105);
    }

    #[test]
    fn a_payload_longer_than_the_maximum_cannot_be_encoded() {
        let mut output = [0; MAX_FRAME];
        let payload = [0u8; MAX_PAYLOAD + 1];
        assert_eq!(
            encode(Direction::FromFlightController, 1, &payload, &mut output),
            Err(Error::PayloadTooLong)
        );
    }

    #[test]
    fn a_short_output_buffer_is_refused_rather_than_truncated() {
        let mut output = [0; 6];
        assert_eq!(
            encode(Direction::FromFlightController, 1, &[1, 2], &mut output),
            Err(Error::OutputTooSmall)
        );
    }
}
