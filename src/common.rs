use std::time::Duration;

use crate::packet::J1939Packet;

/// Represents an adapter. This may be RP1210 or J2534 (eventually)
///
/// Typical use is to log or interogate a vehicle network:
///
/// ```
/// let mut rp1210 = parse.connection.connect()?;
/// let packets = rp1210.iter_for(Duration::from_secs(2));
/// rp1210.send(&J1939Packet::new(1, 0x18EAFFF9, &[0xEC, 0xFE, 0x00]))?;
/// packets
///   .filter(|p| p.pgn() == 0xFEEC )
///   .for_each(|p| println!("VIN: {} packet: {}",String::from_utf8(p.data().to_owned()).unwrap(),p));
///
///  rp1210
///    .iter_for(Duration::from_secs(60 * 60 * 24 * 30))
///    .for_each(|p| println!("{}", p));
/// ```

pub trait Connection: Send + Sync {
    // Send packet on CAN adapter
    fn send(&mut self, packet: &J1939Packet) -> Result<J1939Packet, anyhow::Error>;
    // read packets
    fn iter_for(
        &mut self,
        duration: Duration,
    ) -> Box<dyn Iterator<Item = J1939Packet> + Sync + Send>;
    // echo packet to application, but not CAN adapter
    fn push(&mut self, item: J1939Packet);
}
