use std::time::{Duration, Instant};

use crate::{packet::Packet, sim, slcan};
use anyhow::Result;

#[cfg(windows)]
use crate::rp1210;
#[cfg(target_os = "linux")]
use crate::socketcanconnection;

/// Represents an adapter. This may be RP1210, SocketCAN, simulated or J2534 (eventually)
impl IntoIterator for &mut dyn Connection {
    type Item = Option<Packet>;

    type IntoIter = Box<dyn Iterator<Item = Option<Packet>> + Send + Sync>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl IntoIterator for &dyn Connection {
    type Item = Option<Packet>;

    type IntoIter = Box<dyn Iterator<Item = Option<Packet>> + Send + Sync>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub trait Connection: Send + Sync {
    /// Send packet on CAN adapter
    fn send(&self, packet: &Packet) -> Result<Packet>;

    /// read packets. Some(None) does not indicate end of iterator. Some(None) indicates that a poll() returned None.
    fn iter(&self) -> Box<dyn Iterator<Item = Option<Packet>> + Send + Sync>;

    fn iter_until(&self, end: Instant) -> Box<dyn Iterator<Item = Packet> + Send + Sync> {
        Box::new(self.iter().filter(|o| o.is_some()).map_while(move |o| {
            if Instant::now() > end {
                None
            } else {
                o
            }
        }))
    }

    fn iter_for(&self, duration: Duration) -> Box<dyn Iterator<Item = Packet> + Send + Sync> {
        self.iter_until(Instant::now() + duration)
    }
}

pub trait ConnectionFactory {
    fn create(&self) -> Result<Box<dyn Connection>>;
    fn command_line(&self) -> String;
    fn name(&self) -> String;
}

pub struct ProtocolDescriptor {
    pub name: String,
    pub instructions_url: String,
    pub devices: Vec<DeviceDescriptor>,
}
pub struct DeviceDescriptor {
    pub name: String,
    pub connections: Vec<Box<dyn ConnectionFactory>>,
}

pub fn enumerate_connections() -> Result<Vec<ProtocolDescriptor>> {
    Ok([
        #[cfg(target_os = "windows")]
        rp1210::list_all()?,
        slcan::list_all()?,
        #[cfg(target_os = "linux")]
        socketcanconnection::list_all()?,
        sim::factory()?,
    ]
    // ignore the empty lists
    .into_iter()
    .filter(|c| !c.devices.is_empty())
    .collect())
}
