use std::{fmt::*, ops::Deref};

#[derive(Default, Debug, Clone)]
pub struct Packet {
    pub data: Vec<u8>,
}

#[derive(Default, Debug, Clone)]
pub struct J1939Packet {
    pub packet: Packet,
    pub tx: bool,
    channel: u8,
    time_stamp_weight: f64,
}

impl Deref for J1939Packet {
    type Target = Packet;

    fn deref(&self) -> &Self::Target {
        &self.packet
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
            if self.echo() { " (TX)" } else { "" }
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

impl Display for Packet {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(
            f,
            "{}",
            self.data
                .iter()
                .fold(String::new(), |a, &n| a + &n.to_string() + ", ")
        )
    }
}

impl Packet {
    #[allow(dead_code)]
    pub fn new_rp1210(data: &[u8]) -> Packet {
        Packet {
            data: data.to_vec(),
        }
    }
}

impl J1939Packet {
    #[allow(dead_code)]
    pub fn new_rp1210(channel: u8, data: &[u8], time_stamp_weight: f64) -> J1939Packet {
        J1939Packet {
            packet: Packet::new_rp1210(data),
            tx: false,
            channel,
            time_stamp_weight,
        }
    }

    pub fn len(&self) -> usize {
        self.data.len() - 6 - self.offset()
    }

    #[allow(dead_code)]
    pub fn new_packet(
        channel: u8,
        priority: u8,
        pgn: u32,
        da: u8,
        sa: u8,
        data: &[u8],
    ) -> J1939Packet {
        let da = if pgn >= 0xF000 { 0 } else { da };
        Self::new(
            channel,
            ((priority as u32) << 24)
                | (pgn << 8)
                | if pgn >= 0xf000 { 0 } else { (da as u32) << 8 }
                | (sa as u32),
            data,
        )
    }

    // FIXME use a RP1210 encoder/decoder!
    #[allow(dead_code)]
    pub fn new(channel: u8, head: u32, data: &[u8]) -> J1939Packet {
        let pgn = 0xFFFF & (head >> 8);
        let da = if pgn < 0xF000 { 0xFF & pgn } else { 0 } as u8;
        let hb = head.to_be_bytes();
        let buf = [&[hb[2], hb[1], hb[0] & 0x3, hb[0] >> 2, hb[3], da], data].concat();
        J1939Packet {
            packet: Packet::new_rp1210(&buf),
            tx: true,
            channel,
            time_stamp_weight: 0.0,
        }
    }

    pub fn time(&self) -> f64 {
        if self.tx {
            0.0
        } else {
            let bytes = &self.data[0..4];
            u32::from_be_bytes(bytes.try_into().unwrap()) as f64
                * 0.000001 // convert to s
                * self.time_stamp_weight
        }
    }

    /// offset into array for data common to tx and rx RP1210 formats
    fn offset(&self) -> usize {
        if self.tx {
            0
        } else {
            5
        }
    }

    pub fn echo(&self) -> bool {
        self.tx || self.data[4] != 0
    }

    pub fn source(&self) -> u8 {
        self.data[4 + self.offset()]
    }

    pub fn pgn(&self) -> u32 {
        let mut pgn = ((self.data[2 + self.offset()] as u32 & 0xFF) << 16)
            | ((self.data[1 + self.offset()] as u32 & 0xFF) << 8)
            | (self.data[self.offset()] as u32 & 0xFF);
        if pgn < 0xF000 {
            let destination = self.data[5 + self.offset()] as u32;
            pgn |= destination;
        }
        pgn
    }

    pub fn priority(&self) -> u8 {
        self.data[3 + self.offset()] & 0x07
    }

    pub fn header(&self) -> String {
        format!(
            "{:06X}{:02X}",
            ((self.priority() as u32) << 18) | self.pgn(),
            self.source()
        )
    }

    pub fn id(&self) -> u32 {
        let d = &self.data;
        let o = self.offset();
        let id = u32::from_le_bytes([d[o + 4], d[o], d[o + 1], d[o + 2]]);
        if id > 0xF000 {
            id | ((d[o + 4] as u32) << 8)
        } else {
            id
        }
    }

    pub fn data_str(&self) -> String {
        as_hex(&self.data())
    }

    pub fn data(&self) -> &[u8] {
        &self.data[self.offset() + 6..]
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
            J1939Packet::new(1, 0x18FFAAFA, &[1, 2, 3]).to_string()
        );
        assert_eq!(
            "      0.0000 2 18FFAAF9 [8] 01 02 03 04 05 06 07 08 (TX)",
            J1939Packet::new(2, 0x18FFAAF9, &[1, 2, 3, 4, 5, 6, 7, 8]).to_string()
        );
        assert_eq!(
            "      0.0000 3 18FFAAFB [8] FF 00 FF 00 FF 00 FF 00 (TX)",
            J1939Packet::new(3, 0x18FFAAFB, &[0xFF, 00, 0xFF, 00, 0xFF, 00, 0xFF, 00]).to_string()
        );
    }
}
