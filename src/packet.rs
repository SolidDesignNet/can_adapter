use std::{fmt::*, str::FromStr, time::Duration};

use anyhow::Result;

/// A CAN packet.
#[derive(Debug, Clone)]
pub struct Packet {
    pub id: u32,
    pub payload: Vec<u8>,
    pub state: PacketState,
}

impl FromStr for Packet {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let mut parts = s.split_whitespace();
        let time = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("Missing time"))?;
        let channel = parts.next().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing channel")
        })?;
        let id = parts
            .next()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing id"))?;
        let xmit = parts
            .next()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing xmit"))?;
        let base = parts
            .next()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing base"))?;
        if base != "d" {
            return Err(
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing base").into(),
            );
        }
        let len = parts.next().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing length")
        })?;
        let payload = parts.collect::<Vec<&str>>().join(" ");
        let time = time.parse::<f64>().map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid time: {e}"),
            )
        })?;
        let channel = channel.parse::<u32>().map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid channel: {e}"),
            )
        })?;
        let id = u32::from_str_radix(id.strip_suffix('x').unwrap(), 16).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Invalid id: {e}"))
        })?;
        let len = len.parse::<usize>().map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid length: {e} {len:?}"),
            )
        })?;
        if payload.len() != len * 2 + (len - 1) {
            return Err(anyhow::anyhow!(
                "Payload length does not match length field: {} != {}",
                payload.len(),
                len * 2 + (len - 1)
            ));
        }
        let payload_bytes = payload
            .split_whitespace()
            .map(|b| {
                u8::from_str_radix(b, 16).map_err(|e| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Invalid payload byte: {e}"),
                    )
                })
            })
            .collect::<Result<Vec<u8>, std::io::Error>>()?;
        let state = match xmit {
            "Tx" => Ok(PacketState::TX),
            "Rx" => Ok(PacketState::RX {
                time: Duration::from_secs_f64(time),
                channel,
            }),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid xmit value: {xmit}"),
            )),
        }?;
        Ok(Packet {
            id,
            payload: payload_bytes,
            state,
        })
    }
}

#[derive(Debug, Clone)]
pub enum PacketState {
    TX,
    RX { time: Duration, channel: u32 },
}

/// For now, try to copy the Vector .ASC format to keep the engineering community happy.
impl Display for Packet {
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        write!(
            f,
            "{:12.4} {} {:08X} [{}] {}{}",
            self.time().map(|d| d.as_secs_f64()).unwrap_or_default(),
            self.channel().unwrap_or_default(),
            self.id,
            self.payload.len(),
            self.payload_str(),
            if self.is_tx() { " (TX)" } else { "" }
        )?;
        Ok(())
    }
}

impl Packet {
    /// Creates a new [`Packet`] for transmit.  Applications will use this.
    pub fn new(id: u32, payload: &[u8]) -> Self {
        Self {
            id,
            payload: payload.into(),
            state: PacketState::TX,
        }
    }
    pub fn time(&self) -> Option<Duration> {
        match self.state {
            PacketState::TX => None,
            PacketState::RX { time, channel: _ } => Some(time),
        }
    }
    pub fn channel(&self) -> Option<u32> {
        match self.state {
            PacketState::TX => None,
            PacketState::RX { time: _, channel } => Some(channel),
        }
    }
    pub fn is_tx(&self) -> bool {
        match self.state {
            PacketState::TX => true,
            PacketState::RX {
                time: _,
                channel: _,
            } => false,
        }
    }

    pub fn payload_str_nospace(&self) -> String {
        as_hex_nospace(&self.payload)
    }

    pub fn payload_str(&self) -> String {
        as_hex(&self.payload)
    }

    /// Creates a packet for receive. Connections will call this.
    pub fn new_rx(id: u32, payload: &[u8], time: Duration, channel: u32) -> Packet {
        Packet {
            id,
            payload: payload.into(),
            state: PacketState::RX { time, channel },
        }
    }
    pub(crate) fn len(&self) -> usize {
        self.payload.len()
    }
}

fn as_hex(data: &[u8]) -> String {
    data.iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<String>>()
        .join(" ")
}

fn as_hex_nospace(data: &[u8]) -> String {
    // FIXME optimize
    let mut s = String::with_capacity(data.len() * 2);
    for byte in data {
        write!(&mut s, "{byte:02X}").expect("Unable to write");
    }
    s.to_string()
}
