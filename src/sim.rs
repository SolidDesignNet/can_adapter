use anyhow::*;
use std::sync::atomic::*;
use std::sync::*;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::common::Connection;
use crate::bus::{Bus, PushBus};
use crate::packet::*;

pub struct Rp1210 {
    bus: Box<dyn Bus<J1939Packet>>,
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
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
        let mut bus = PushBus::new();
        let running = Arc::new(AtomicBool::new(false));
        let dev = device as u8;
        let rp1210 = Rp1210 {
            bus: bus.clone_bus(),
            running: running.clone(),
            thread: Some(std::thread::spawn(move || {
                running.store(true, Ordering::Relaxed);
                let mut seq: u64 = u64::from_be_bytes([dev, 0, 0, 0, 0, 0, 0, 0]);
                while running.load(Ordering::Relaxed) {
                    bus.push(J1939Packet::new_packet(
                        channel.unwrap_or(0),
                        6,
                        0xFFFF,
                        0,
                        0xF9,
                        &seq.to_be_bytes(),
                    ));
                    std::thread::sleep(Duration::from_millis(10));
                    seq = seq + 1;
                }
            })),
        };
        Ok(rp1210)
    }
}
impl Connection for Rp1210 {
    /// Send packet and return packet echoed back from adapter
    fn send(&mut self, packet: &J1939Packet) -> Result<J1939Packet> {
        let p = packet.clone();
        p.time();
        self.bus.push(p.clone());
        Ok(p)
    }

    fn iter_for(&mut self, duration: Duration) -> Box<dyn Iterator<Item = J1939Packet>+Send+Sync> {
        self.bus.iter_for(duration)
    }

    fn push(&mut self, item: J1939Packet) {
        self.bus.push(item);
    }
}

impl Drop for Rp1210 {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        let _ = self.thread.take().unwrap().join();
    }
}
