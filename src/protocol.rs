//! Zwift Ride controller protocol.
//!
//! Learned from BikeControl's open-source code (github.com/jonasbark/swiftcontrol):
//! subscribe to the async + sync-tx characteristics, write `RideOn` to sync-rx,
//! then each notification starting with 0x23 carries a `RideKeyPadStatus` protobuf.

use uuid::Uuid;

pub const ZWIFT_MANUFACTURER_ID: u16 = 0x094A;
pub const RIDE_RIGHT: u8 = 0x07;
pub const RIDE_LEFT: u8 = 0x08;

pub const RIDE_SERVICE: Uuid = Uuid::from_u128(0x0000fc82_0000_1000_8000_00805f9b34fb);
pub const LEGACY_SERVICE: Uuid = Uuid::from_u128(0x00000001_19ca_4651_86e5_fa29dcdd09d1);
pub const ASYNC_CHAR: Uuid = Uuid::from_u128(0x00000002_19ca_4651_86e5_fa29dcdd09d1);
pub const SYNC_RX_CHAR: Uuid = Uuid::from_u128(0x00000003_19ca_4651_86e5_fa29dcdd09d1);
pub const SYNC_TX_CHAR: Uuid = Uuid::from_u128(0x00000004_19ca_4651_86e5_fa29dcdd09d1);

pub const RIDE_ON: &[u8] = b"RideOn";
pub const RIDE_NOTIFICATION: u8 = 0x23;
pub const PADDLE_THRESHOLD: u64 = 25;

/// Every Ride button, in config order: (config name, bit in buttonMap; bit is 0 while pressed).
/// Paddles are analog and have no bit.
pub const BUTTONS: &[(&str, Option<u32>)] = &[
    ("up", Some(0x00002)),
    ("down", Some(0x00008)),
    ("left", Some(0x00001)),
    ("right", Some(0x00004)),
    ("a", Some(0x00010)),
    ("b", Some(0x00020)),
    ("y", Some(0x00040)),
    ("z", Some(0x00080)),
    ("shift_up_right", Some(0x01000)),
    ("shift_down_right", Some(0x02000)),
    ("shift_up_left", Some(0x00100)),
    ("shift_down_left", Some(0x00200)),
    ("powerup_right", Some(0x04000)),
    ("powerup_left", Some(0x00400)),
    ("onoff_right", Some(0x08000)),
    ("onoff_left", Some(0x00800)),
    ("paddle_left", None),
    ("paddle_right", None),
];

/// Names of the buttons currently pressed in a notification, or None if it isn't a button message.
pub fn pressed_buttons(data: &[u8]) -> Option<Vec<&'static str>> {
    if data.first() != Some(&RIDE_NOTIFICATION) {
        return None;
    }
    let (button_map, paddles) = parse_status(&data[1..])?;
    let mut pressed: Vec<&str> = BUTTONS
        .iter()
        .filter_map(|(name, bit)| bit.filter(|b| button_map & b == 0).map(|_| *name))
        .collect();
    for (location, magnitude) in paddles {
        if magnitude >= PADDLE_THRESHOLD {
            match location {
                0 => pressed.push("paddle_left"),
                1 => pressed.push("paddle_right"),
                _ => {}
            }
        }
    }
    Some(pressed)
}

fn read_varint(buf: &[u8], i: &mut usize) -> Option<u64> {
    let mut val = 0u64;
    for shift in (0..64).step_by(7) {
        let b = *buf.get(*i)?;
        *i += 1;
        val |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Some(val);
        }
    }
    None
}

/// `RideKeyPadStatus`: field 1 = buttonMap, field 3 = repeated { 1: location, 2: analog value }.
fn parse_status(buf: &[u8]) -> Option<(u32, Vec<(u64, u64)>)> {
    let mut button_map = None;
    let mut paddles = Vec::new();
    let mut i = 0;
    while i < buf.len() {
        let tag = read_varint(buf, &mut i)?;
        match (tag >> 3, tag & 7) {
            (field, 0) => {
                let v = read_varint(buf, &mut i)?;
                if field == 1 {
                    button_map = Some(v as u32);
                }
            }
            (field, 2) => {
                let len = read_varint(buf, &mut i)? as usize;
                let sub = buf.get(i..i + len)?;
                i += len;
                if field == 3 {
                    paddles.push(parse_paddle(sub)?);
                }
            }
            (_, 1) => i += 8,
            (_, 5) => i += 4,
            _ => return None,
        }
    }
    Some((button_map?, paddles))
}

fn parse_paddle(buf: &[u8]) -> Option<(u64, u64)> {
    let (mut location, mut raw, mut i) = (0, 0, 0);
    while i < buf.len() {
        let tag = read_varint(buf, &mut i)?;
        if tag & 7 != 0 {
            return None;
        }
        let v = read_varint(buf, &mut i)?;
        match tag >> 3 {
            1 => location = v,
            2 => raw = v,
            _ => {}
        }
    }
    // The left paddle pulls negative: as int32 that's a huge varint, as sint32 a zigzag value.
    // Taking the two's-complement magnitude covers the first; the threshold still works for the second.
    let magnitude = if raw >= 1 << 63 {
        (raw as i64).unsigned_abs()
    } else if raw >= 1 << 31 {
        u64::from((raw as u32 as i32).unsigned_abs())
    } else {
        raw
    };
    Some((location, magnitude))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn varint(mut v: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                return out;
            }
            out.push(b | 0x80);
        }
    }

    fn message(button_map: u32, paddles: &[(u64, i64)]) -> Vec<u8> {
        let mut m = vec![RIDE_NOTIFICATION, 0x08];
        m.extend(varint(button_map as u64));
        for &(loc, val) in paddles {
            let mut sub = vec![0x08];
            sub.extend(varint(loc));
            sub.push(0x10);
            sub.extend(varint(val as u64));
            m.push(0x1a);
            m.extend(varint(sub.len() as u64));
            m.extend(sub);
        }
        m
    }

    #[test]
    fn nothing_pressed() {
        assert_eq!(pressed_buttons(&message(0xFFFF_FFFF, &[])), Some(vec![]));
    }

    #[test]
    fn buttons_are_active_low() {
        let m = message(!(0x10 | 0x02), &[]);
        assert_eq!(pressed_buttons(&m), Some(vec!["up", "a"]));
    }

    #[test]
    fn paddles_both_directions() {
        let m = message(0xFFFF_FFFF, &[(0, -100), (1, 100)]);
        assert_eq!(pressed_buttons(&m), Some(vec!["paddle_left", "paddle_right"]));
        let light = message(0xFFFF_FFFF, &[(0, -10), (1, 10)]);
        assert_eq!(pressed_buttons(&light), Some(vec![]));
    }

    #[test]
    fn ignores_other_messages() {
        assert_eq!(pressed_buttons(&[0x19, 0x08, 0x50]), None);
        assert_eq!(pressed_buttons(&[]), None);
    }
}
