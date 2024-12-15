use std::time::Duration;

use crate::packet::J1939Packet;

pub trait Connection {
    fn send(&mut self, packet: &J1939Packet) -> Result<J1939Packet, anyhow::Error>;
    fn iter_for(&self, duration: Duration) -> impl Iterator<Item = J1939Packet>;
    fn push(&mut self, item: J1939Packet);
}
