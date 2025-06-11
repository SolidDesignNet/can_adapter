use crate::{
    connection::Connection,
    packet::J1939PacketX,
};

struct J1939TP<C: Connection> {
    connection: C,
}
impl Connection for J1939TP<dyn Connection<Packet = J1939PacketX>> {
    type Packet = J1939PacketX;
    fn send(
        &mut self,
        packet: &crate::packet::J1939PacketX,
    ) -> anyhow::Result<crate::packet::J1939PacketX, anyhow::Error> {
        self.connection.send(packet)
    }

    fn iter(&self) -> Box<dyn Iterator<Item = Option<crate::packet::J1939PacketX>> + Send + Sync> {
        todo!()
    }
}
