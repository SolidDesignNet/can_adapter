use std::time::{Duration, Instant};

use crate::packet::J1939Packet;

/// Represents an adapter. This may be RP1210 or J2534 (eventually)
///
/// Typical use is to log or interogate a vehicle network:
///
/// ```
/// use std::time::{Duration, Instant};
/// use can_adapter::connection::Connection;
/// use can_adapter::packet::J1939Packet;
/// fn vin(rp1210: & mut dyn Connection) ->Result<(),anyhow::Error> {
///   let packets = rp1210.iter_for(Duration::from_secs(2));
///   rp1210.send(&J1939Packet::new(1, 0x18EAFFF9, &[0xEC, 0xFE, 0x00]))?;
///   packets
///     .filter(|p| p.pgn() == 0xFEEC )
///     .for_each(|p| println!("VIN: {} packet: {}",String::from_utf8(p.data().to_owned()).unwrap(),p));
///  
///    rp1210
///      .iter_for(Duration::from_secs(60 * 60 * 24 * 30))
///      .for_each(|p| println!("{}", p));
///    Ok(())
/// }
/// ```

pub trait Connection: Send + Sync {
    // Send packet on CAN adapter
    fn send(&mut self, packet: &J1939Packet) -> Result<J1939Packet, anyhow::Error>;

    // read packets. Some(None) does not indicate end of iterator. Some(None) indicates that a poll() returned None.
    fn iter(&self) -> Box<dyn Iterator<Item = Option<J1939Packet>> + Send + Sync>;
    
    fn iter_until(&self, end: Instant) -> Box<dyn Iterator<Item = J1939Packet> + Send + Sync> {
        Box::new(self.iter().filter(|o| o.is_some()).map_while(move |o| {
            if Instant::now() > end {
                None
            } else {
                o
            }
        }))
    }
    fn iter_for(&self, duration: Duration) -> Box<dyn Iterator<Item = J1939Packet> + Send + Sync> {
        self.iter_until(Instant::now() + duration)
    }
}
