use std::time::Duration;

use crate::packet::J1939Packet;

pub trait Connection {
    // Send packet on CAN adapter
    fn send(&mut self, packet: &J1939Packet) -> Result<J1939Packet, anyhow::Error>;
    // read packets
    fn iter_for(&mut self, duration: Duration) -> Box<dyn  Iterator<Item = J1939Packet>>;
    // echo packet to application, but not CAN adapter
    fn push(&mut self, item: J1939Packet);
}
pub trait Bus<T>: Send
where
    T: Clone,
{
    /// used to read packets from the bus for a duration (typically considered a response timeout).
    fn iter_for(&mut self, duration: Duration) -> Box<dyn Iterator<Item = T>>;
    fn push(&mut self, item: T);
    fn clone_bus(&self) -> Box<dyn Bus<T>>;
}
