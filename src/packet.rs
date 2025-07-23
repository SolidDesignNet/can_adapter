use std::{fmt::*, time::Duration};

#[derive(Debug, Clone)]
pub struct Packet {
    pub id: u32,
    pub payload: Vec<u8>,
    pub state: PacketState,
}

#[derive(Debug, Clone)]
enum PacketState {
    TX,
    RX { time: Duration, channel: u32 },
}

/// For now, try to copy the Vector .ASC format to keep the engineering community happy.
impl Display for Packet {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(
            f,
            "{:12.4} {} {:X} [{}] {}{}",
            self.time().map(|d| d.as_secs_f64()).unwrap_or_default(),
            self.channel().unwrap_or_default(),
            self.id,
            self.payload.len(),
            self.payload_str(),
            if self.is_tx() { " (TX)" } else { "" }
        )
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
            PacketState::RX { time, channel } => Some(time),
        }
    }
    pub fn channel(&self) -> Option<u32> {
        match self.state {
            PacketState::TX => None,
            PacketState::RX { time, channel } => Some(channel),
        }
    }
    pub fn is_tx(&self) -> bool {
        match self.state {
            PacketState::TX => true,
            PacketState::RX { time, channel } => false,
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
    s.to_string()
}
