use std::time::Duration;

use crate::packet::J1939Packet;

pub trait Connection {
    // Send packet on CAN adapter
    fn send(&mut self, packet: &J1939Packet) -> Result<J1939Packet, anyhow::Error>;
    // read packets
    fn iter_for(&mut self, duration: Duration) -> impl Iterator<Item = J1939Packet>;
    // echo packet to application, but not CAN adapter
    fn push(&mut self, item: J1939Packet);
}
