use std::fmt::*;

#[derive(Default, Clone)]
pub struct J1939Packet {
    id: u32,
    payload: Vec<u8>,
    tx: bool,
    channel: u8,
    time_stamp_weight: f64,
    time: u32,
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
    let mut s = String::new();
    for byte in data {
        write!(&mut s, " {:02X}", byte).expect("Unable to write");
    }
    s[1..].to_string()
}

impl J1939Packet {
    pub fn new_rp1210(tx: bool, channel: u8, data: &[u8], time_stamp_weight: f64) -> J1939Packet {
        let (time, data) = if tx {
            (u32::from_be_bytes(data[0..4].try_into().expect("")), &data[5..])
        } else {
            (0, &data[0..])
        };
        let payload: Vec<u8> = data[4..].into();
        let priority = data[0] as u32;
        let pgn = u32::from_be_bytes([0, data[1], data[2], data[3]]);
        let sa = data[4] as u32;
        J1939Packet {
            id: (priority << 27) | (pgn << 8) | sa,
            payload,
            tx,
            channel,
            time_stamp_weight,
            time,
        }
    }

    pub fn len(&self) -> usize {
        self.payload.len()
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
            ((priority as u32) << 24)
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
        (self.id >> 27) as u8
    }

    pub fn header(&self) -> String {
        format!("{:08X}", self.id)
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    fn data_str(&self) -> String {
        as_hex(&self.data())
    }

    pub fn data(&self) -> &[u8] {
        &self.payload
    }

    pub fn channel(&self) -> u8 {
        self.channel
    }
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_j1939packet_display() {
        assert_eq!(
            "      0.0000 1 18FFAAFA [3] 01 02 03 (TX)",
            J1939Packet::new(None, 1, 0x18FFAAFA, &[1, 2, 3]).to_string()
        );
        assert_eq!(
            "      0.0000 1 18FFAAFA [3] 01 02 03 (TX)",
            J1939Packet::new(Some(555), 1, 0x18FFAAFA, &[1, 2, 3]).to_string()
        );
        assert_eq!(
            "      0.0000 2 18FFAAF9 [8] 01 02 03 04 05 06 07 08 (TX)",
            J1939Packet::new(None, 2, 0x18FFAAF9, &[1, 2, 3, 4, 5, 6, 7, 8]).to_string()
        );
        assert_eq!(
            "      0.0000 3 18FFAAFB [8] FF 00 FF 00 FF 00 FF 00 (TX)",
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
