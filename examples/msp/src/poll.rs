//! Replies to the requests a video transmitter sends before it will show an
//! MSP canvas.
//!
//! The transmitter is not a passive display. It asks what it is connected to,
//! and a controller that stays silent gets the transmitter's own OSD instead of
//! the one it is trying to draw. The set answered here is the identity handful
//! plus the telemetry a stock OSD layout reads.

use crate::codec::{Direction, MAX_FRAME, Packet, encode};

pub const API_VERSION: u8 = 1;
pub const FC_VARIANT: u8 = 2;
pub const FC_VERSION: u8 = 3;
pub const NAME: u8 = 10;
pub const STATUS: u8 = 101;
pub const RC: u8 = 105;
pub const ANALOG: u8 = 110;
pub const BATTERY_STATE: u8 = 130;
pub const STATUS_EX: u8 = 150;

/// How many channels [`Telemetry`] carries. `MSP_RC` is variable-length; eight
/// is what a transmitter needs to show sticks and an arm switch.
pub const CHANNELS: usize = 8;

/// Identifying as Betaflight is not vanity. A transmitter branches on the
/// variant string to decide which OSD dialect to speak, and every third-party
/// controller that wants the common path answers this way.
const VARIANT: &[u8; 4] = b"BTFL";

/// What a transmitter is told when it asks.
///
/// Channels are in microseconds, the units `MSP_RC` is defined in - not raw
/// receiver units. Converting is the receiver's business, and a different
/// protocol would convert differently.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Telemetry {
    pub channels: [u16; CHANNELS],
    pub armed: bool,
}

impl Telemetry {
    pub const fn new() -> Self {
        Self {
            channels: [1500; CHANNELS],
            armed: false,
        }
    }
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds the reply to one request, or `None` if the packet was not addressed
/// to a flight controller.
///
/// An unrecognized command is answered with MSP's error direction rather than
/// an empty success. That is what a stock controller does, so it is the path
/// every transmitter is tested against; answering an empty success instead
/// tells the transmitter a feature exists and returned nothing.
pub fn respond(
    request: &Packet,
    telemetry: &Telemetry,
    output: &mut [u8; MAX_FRAME],
) -> Option<usize> {
    if request.direction != Direction::ToFlightController {
        return None;
    }

    let mut payload = [0u8; 32];
    let length = match request.command {
        API_VERSION => {
            payload[..3].copy_from_slice(&[0, 1, 46]);
            Some(3)
        }
        FC_VARIANT => {
            payload[..4].copy_from_slice(VARIANT);
            Some(4)
        }
        FC_VERSION => {
            payload[..3].copy_from_slice(&[4, 5, 0]);
            Some(3)
        }
        NAME => {
            let name = b"FERROFORGE";
            payload[..name.len()].copy_from_slice(name);
            Some(name.len())
        }
        STATUS => Some(write_status(&mut payload, telemetry, false)),
        STATUS_EX => Some(write_status(&mut payload, telemetry, true)),
        RC => Some(write_rc(&mut payload, telemetry)),
        ANALOG => Some(write_analog(&mut payload)),
        BATTERY_STATE => Some(write_battery_state(&mut payload)),
        _ => None,
    };

    match length {
        Some(length) => encode(
            Direction::FromFlightController,
            request.command,
            &payload[..length],
            output,
        )
        .ok(),
        None => encode(Direction::Failed, request.command, &[], output).ok(),
    }
}

fn put_u16(output: &mut [u8], offset: usize, value: u16) {
    output[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(output: &mut [u8], offset: usize, value: u32) {
    output[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// Cycle time, I2C errors, a sensor bitmap, the flight-mode flags and the
/// active profile. Only the arming flag carries information here; the rest is
/// the shape the request has.
fn write_status(output: &mut [u8], telemetry: &Telemetry, extended: bool) -> usize {
    put_u16(output, 0, 2500);
    put_u16(output, 2, 0);
    output[4] = 0x01;
    output[5] = 0;
    put_u32(output, 6, telemetry.armed as u32);
    output[10] = 0;
    if !extended {
        return 11;
    }
    put_u16(output, 11, 5);
    output[13] = 3;
    output[14] = 0;
    15
}

fn write_rc(output: &mut [u8], telemetry: &Telemetry) -> usize {
    for (index, value) in telemetry.channels.iter().enumerate() {
        put_u16(output, index * 2, *value);
    }
    telemetry.channels.len() * 2
}

/// Voltage, consumption, RSSI and current, none of which this board measures.
fn write_analog(output: &mut [u8]) -> usize {
    output[0] = 0;
    put_u16(output, 1, 0);
    put_u16(output, 3, 0);
    put_u16(output, 5, 0);
    7
}

fn write_battery_state(output: &mut [u8]) -> usize {
    output[0] = 0;
    put_u16(output, 1, 0);
    output[3] = 0;
    put_u16(output, 4, 0);
    put_u16(output, 6, 0);
    output[8] = 0;
    9
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::Parser;

    fn request(command: u8) -> Packet {
        let mut bytes = [0; MAX_FRAME];
        let len = encode(Direction::ToFlightController, command, &[], &mut bytes).unwrap();
        let mut parser = Parser::new();
        let mut packet = None;
        for byte in &bytes[..len] {
            packet = parser.push(*byte).unwrap().or(packet);
        }
        packet.unwrap()
    }

    fn reply(command: u8, telemetry: &Telemetry) -> Packet {
        let mut output = [0; MAX_FRAME];
        let len = respond(&request(command), telemetry, &mut output).expect("a reply");
        let mut parser = Parser::new();
        let mut packet = None;
        for byte in &output[..len] {
            packet = parser.push(*byte).unwrap().or(packet);
        }
        packet.unwrap()
    }

    #[test]
    fn the_identity_requests_are_answered() {
        let telemetry = Telemetry::new();
        assert_eq!(reply(FC_VARIANT, &telemetry).payload(), b"BTFL");
        assert_eq!(reply(NAME, &telemetry).payload(), b"FERROFORGE");
        assert_eq!(reply(API_VERSION, &telemetry).payload(), &[0, 1, 46]);
    }

    #[test]
    fn every_reply_is_addressed_back_to_the_transmitter() {
        let telemetry = Telemetry::new();
        for command in [API_VERSION, FC_VARIANT, FC_VERSION, NAME, STATUS, RC] {
            let packet = reply(command, &telemetry);
            assert_eq!(packet.direction, Direction::FromFlightController);
            assert_eq!(packet.command, command);
        }
    }

    #[test]
    fn channels_come_back_little_endian_in_request_order() {
        let mut telemetry = Telemetry::new();
        telemetry.channels[0] = 1000;
        telemetry.channels[1] = 2000;
        telemetry.channels[4] = 1234;

        let packet = reply(RC, &telemetry);
        assert_eq!(packet.payload().len(), CHANNELS * 2);
        assert_eq!(&packet.payload()[0..2], &1000u16.to_le_bytes());
        assert_eq!(&packet.payload()[2..4], &2000u16.to_le_bytes());
        assert_eq!(&packet.payload()[8..10], &1234u16.to_le_bytes());
    }

    #[test]
    fn arming_shows_up_in_the_status_flags() {
        let mut telemetry = Telemetry::new();
        assert_eq!(&reply(STATUS, &telemetry).payload()[6..10], &[0, 0, 0, 0]);
        telemetry.armed = true;
        assert_eq!(&reply(STATUS, &telemetry).payload()[6..10], &[1, 0, 0, 0]);
    }

    #[test]
    fn the_extended_status_is_longer_and_agrees_up_to_where_it_grows() {
        let telemetry = Telemetry::new();
        let short = reply(STATUS, &telemetry);
        let long = reply(STATUS_EX, &telemetry);
        assert_eq!(short.payload().len(), 11);
        assert_eq!(long.payload().len(), 15);
        assert_eq!(short.payload(), &long.payload()[..11]);
    }

    /// The behaviour a stock controller has, and so the one a transmitter is
    /// tested against.
    #[test]
    fn an_unknown_command_is_refused_rather_than_answered_empty() {
        let telemetry = Telemetry::new();
        let packet = reply(77, &telemetry);
        assert_eq!(packet.direction, Direction::Failed);
        assert_eq!(packet.payload(), b"");
    }

    /// A reply is a reply; answering one would talk over the transmitter.
    #[test]
    fn a_packet_from_a_flight_controller_is_not_answered() {
        let mut bytes = [0; MAX_FRAME];
        let len = encode(Direction::FromFlightController, STATUS, &[], &mut bytes).unwrap();
        let mut parser = Parser::new();
        let mut packet = None;
        for byte in &bytes[..len] {
            packet = parser.push(*byte).unwrap().or(packet);
        }
        let mut output = [0; MAX_FRAME];
        assert_eq!(
            respond(&packet.unwrap(), &Telemetry::new(), &mut output),
            None
        );
    }
}
