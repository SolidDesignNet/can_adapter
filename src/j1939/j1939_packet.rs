use std::{fmt::*, ops::Deref, time::Duration};

use crate::packet::Packet;

#[derive(Clone)]
pub struct J1939Packet {
    packet: Packet,
}

impl From<Packet> for J1939Packet {
    fn from(value: Packet) -> Self {
        J1939Packet { packet: value }
    }
}
impl From<&Packet> for J1939Packet {
    fn from(value: &Packet) -> Self {
        J1939Packet {
            packet: value.clone(),
        }
    }
}
impl From<J1939Packet> for Packet {
    fn from(value: J1939Packet) -> Self {
        value.packet
    }
}
impl From<&J1939Packet> for Packet {
    fn from(value: &J1939Packet) -> Self {
        value.packet.clone()
    }
}
impl Deref for J1939Packet {
    type Target = Packet;

    fn deref(&self) -> &Self::Target {
        &self.packet
    }
}

impl J1939Packet {
    pub fn new_packet(
        time: Option<Duration>,
        channel: u32,
        priority: u8,
        pgn: u32,
        da: u8,
        sa: u8,
        data: &[u8],
    ) -> J1939Packet {
        let da = if pgn >= 0xF000 { 0 } else { da };
        {
            let id = ((priority as u32) << 26)
                | (pgn << 8)
                | if pgn >= 0xf000 { 0 } else { (da as u32) << 8 }
                | (sa as u32);
            J1939Packet {
                packet: if let Some(time1) = time {
                    Packet::new_rx(id, data, time1, channel)
                } else {
                    Packet::new(id, data)
                },
            }
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

    pub(crate) fn data(&self) -> &[u8] {
        &self.payload
    }

    pub fn new(id: u32, payload: &[u8]) -> Self {
        Self {
            packet: Packet::new(id, payload),
        }
    }
}

impl Debug for J1939Packet {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Debug::fmt(&self.packet, f)
    }
}
impl Display for J1939Packet {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Display::fmt(&self.packet, f)
    }
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_j1939packet_display() {
        // channel is ignored on TX. The channel is set in `connection.send()`
        assert_eq!(
            "      0.0000 0 18FFAAFA [3] 01 02 03 (TX)",
            Packet::new(0x18FFAAFA, &[1, 2, 3]).to_string()
        );

        assert_eq!(
            "      0.5550 1 18FFAAFA [3] 01 02 03",
            Packet::new_rx(0x18FFAAFA, &[1, 2, 3], Duration::new(0, 555_000_000), 1,).to_string()
        );
        assert_eq!(
            "      0.0000 0 18FFAAF9 [8] 01 02 03 04 05 06 07 08 (TX)",
            Packet::new(0x18FFAAF9, &[1, 2, 3, 4, 5, 6, 7, 8]).to_string()
        );
        assert_eq!(
            "      0.0000 0 18FFAAFB [8] FF 00 FF 00 FF 00 FF 00 (TX)",
            Packet::new(0x18FFAAFB, &[0xFF, 00, 0xFF, 00, 0xFF, 00, 0xFF, 00]).to_string()
        );
        assert_eq!(
            "      0.0000 0 0CFFAAFB [8] FF 00 FF 00 FF 00 FF 00 (TX)",
            Packet::new(0x0CFFAAFB, &[0xFF, 00, 0xFF, 00, 0xFF, 00, 0xFF, 00]).to_string()
        );
    }
}
