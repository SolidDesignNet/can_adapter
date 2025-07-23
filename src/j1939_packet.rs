use std::fmt::*;

use crate::packet::Packet;

#[derive(Default, Clone)]
pub struct J1939Packet {
    id: u32,
    payload: Vec<u8>,
    tx: bool,
    channel: u8,
    time_stamp_weight: f64,
    time: u32,
}

impl From<Packet> for J1939Packet {
    fn from(value: Packet) -> Self {
        J1939Packet::from(&value)
    }
}
impl From<&Packet> for J1939Packet {
    fn from(value: &Packet) -> Self {
        J1939Packet {
            id: value.id,
            payload: value.payload.clone(),
            tx: value.tx,
            channel: 0,
            time_stamp_weight: 1.0,
            time: value.time as u32,
        }
    }
}

impl From<J1939Packet> for Packet {
    fn from(value: J1939Packet) -> Self {
        Packet::from(&value)
    }
}
impl From<&J1939Packet> for Packet {
    fn from(value: &J1939Packet) -> Self {
        Packet {
            id: value.id,
            payload: value.payload.clone(),
            tx: value.tx,
            time: value.time_stamp_weight * (value.time as f64),
        }
    }
}
impl Debug for J1939Packet {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Display::fmt(&self, f)
    }
}
impl Display for J1939Packet {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(
            f,
            "{:12.4} {} {} [{}] {}{}",
            self.time(),
            self.channel(),
            self.header(),
            self.len(),
            self.data_str(),
            if self.tx { " (TX)" } else { "" }
        )
    }
}

fn as_hex(data: &[u8]) -> String {
    if data.is_empty() {
        return "".to_string();
    }
    // FIXME optimize
    let mut s = String::new();
    for byte in data {
        write!(&mut s, " {byte:02X}").expect("Unable to write");
    }
    s[1..].to_string()
}
fn as_hex_nospace(data: &[u8]) -> String {
    // FIXME optimize
    let mut s = String::new();
    for byte in data {
        write!(&mut s, "{byte:02X}").expect("Unable to write");
    }
    s
}

impl J1939Packet {
    pub fn len(&self) -> usize {
        self.payload.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn time(&self) -> u32 {
        self.time
    }

    pub fn new_packet(
        time: Option<u32>,
        channel: u8,
        priority: u8,
        pgn: u32,
        da: u8,
        sa: u8,
        data: &[u8],
    ) -> J1939Packet {
        let da = if pgn >= 0xF000 { 0 } else { da };
        Self::new(
            time,
            channel,
            ((priority as u32) << 26)
                | (pgn << 8)
                | if pgn >= 0xf000 { 0 } else { (da as u32) << 8 }
                | (sa as u32),
            data,
        )
    }

    pub fn new(time: Option<u32>, channel: u8, id: u32, payload: &[u8]) -> J1939Packet {
        J1939Packet {
            id,
            payload: payload.into(),
            tx: time.is_none(),
            channel,
            time_stamp_weight: 1000.0,
            time: time.unwrap_or(0u32),
        }
    }

    pub fn new_socketcan(time: u32, tx: bool, id: u32, payload: &[u8]) -> J1939Packet {
        J1939Packet {
            id,
            payload: payload.into(),
            tx,
            channel: 0,
            time_stamp_weight: 1000.0,
            time,
        }
    }

    pub fn source(&self) -> u8 {
        (self.id & 0xFF) as u8
    }

    pub fn pgn(&self) -> u32 {
        let mut pgn = 0xFFFF & (self.id >> 8);
        if pgn < 0xF000 {
            pgn |= self.dest() as u32;
        }
        pgn
    }

    pub fn dest(&self) -> u8 {
        (0xFF & (self.id >> 8)) as u8
    }

    pub fn priority(&self) -> u8 {
        (self.id >> 26) as u8
    }

    pub fn header(&self) -> String {
        format!("{:08X}", self.id)
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn data_str(&self) -> String {
        as_hex(self.data())
    }

    pub fn data_str_nospace(&self) -> String {
        as_hex_nospace(self.data())
    }

    pub fn data(&self) -> &[u8] {
        &self.payload
    }

    pub fn channel(&self) -> u8 {
        self.channel
    }

    pub fn time_stamp_weight(&self) -> f64 {
        self.time_stamp_weight
    }
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_j1939packet_display() {
        assert_eq!(
            "           0 1 18FFAAFA [3] 01 02 03 (TX)",
            J1939Packet::new(None, 1, 0x18FFAAFA, &[1, 2, 3]).to_string()
        );
        assert_eq!(
            "         555 1 18FFAAFA [3] 01 02 03",
            J1939Packet::new(Some(555), 1, 0x18FFAAFA, &[1, 2, 3]).to_string()
        );
        assert_eq!(
            "           0 2 18FFAAF9 [8] 01 02 03 04 05 06 07 08 (TX)",
            J1939Packet::new(None, 2, 0x18FFAAF9, &[1, 2, 3, 4, 5, 6, 7, 8]).to_string()
        );
        assert_eq!(
            "           0 3 18FFAAFB [8] FF 00 FF 00 FF 00 FF 00 (TX)",
            J1939Packet::new(
                None,
                3,
                0x18FFAAFB,
                &[0xFF, 00, 0xFF, 00, 0xFF, 00, 0xFF, 00]
            )
            .to_string()
        );
    }
}
