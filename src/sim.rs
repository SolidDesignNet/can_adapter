use anyhow::*;
use std::sync::atomic::*;
use std::sync::*;
use std::thread::Builder;
use std::time::Duration;

use crate::bus::{Bus, PushBus};
use crate::connection::Connection;
use crate::packet::*;

pub struct Rp1210 {
    bus: Box<PushBus<J1939Packet>>,
    running: Arc<AtomicBool>,
}
impl Rp1210 {
    #[deprecated(note = "Must be built with Win32 target to use RP1210 adapters.")]
    pub fn new(
        _id: &str,
        device: i16,
        channel: Option<u8>,
        _connection_string: &str,
        _address: u8,
        _app_packetized: bool,
    ) -> Result<Rp1210> {
        let bus = PushBus::new();
        let running = Arc::new(AtomicBool::new(false));
        let dev = device as u8;
        {
            let running = running.clone();
            let mut bus = bus.clone();
            Builder::new().name("rp1210".into()).spawn(move || {
                running.store(true, Ordering::Relaxed);
                let mut seq: u64 = u64::from_be_bytes([dev, 0, 0, 0, 0, 0, 0, 0]);
                while running.load(Ordering::Relaxed) {
                    let packet = J1939Packet::new_packet(
                        channel.unwrap_or(0),
                        6,
                        0xFFFF,
                        0,
                        0xF9,
                        &seq.to_be_bytes(),
                    );
                    bus.push(Some(packet));
                    std::thread::sleep(Duration::from_millis(100));
                    seq = seq + 1;
                }
            })?;
        }
        Ok(Rp1210 {
            bus: Box::new(bus.clone()),
            running: running.clone(),
        })
    }
}

impl Connection for Rp1210 {
    /// Send packet and return packet echoed back from adapter
    fn send(&mut self, packet: &J1939Packet) -> Result<J1939Packet> {
        let p = packet.clone();
        p.time();
        self.bus.push(Some(p.clone()));
        Ok(p)
    }

    fn iter(&self) -> Box<dyn Iterator<Item = Option<J1939Packet>> + Send + Sync> {
        self.bus.iter()
    }
}

impl Drop for Rp1210 {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        self.bus.close();
        //let _ = self.thread.take().unwrap().join();
    }
}
