use anyhow::*;
use std::sync::atomic::*;
use std::sync::*;
use std::time::Duration;

use crate::multiqueue::*;
use crate::packet::*;

pub struct Rp1210 {
    pub bus: MultiQueue<J1939Packet>,
    pub running: Arc<AtomicBool>,
    pub id: String,
    pub device: i16,
    pub connection_string: String,
}
impl Rp1210 {
    #[deprecated(note = "Must be built with Win32 target to use RP1210 adapters.")]
    pub fn new(
        id: &str,
        device: i16,
        connection_string: &str,
        _address: u8,
        bus: MultiQueue<J1939Packet>,
    ) -> Result<Rp1210> {
        Ok(Rp1210 {
            bus,
            running: Arc::new(AtomicBool::new(false)),
            id: id.to_owned(),
            device,
            connection_string: connection_string.to_owned(),
        })
    }
    /// background thread to read all packets into queue
    pub fn run(&mut self) -> std::thread::JoinHandle<()> {
        let mut bus = self.bus.clone();
        let running = self.running.clone();
        std::thread::spawn(move || {
            running.store(true, Ordering::Relaxed);
            let mut seq: u64 = 0;
            while running.load(Ordering::Relaxed) {
                bus.push(J1939Packet::new_packet(
                    6,
                    0xFFFF,
                    0,
                    0xF9,
                    &seq.to_be_bytes(),
                ));
                std::thread::sleep(Duration::from_millis(10));
                seq = seq + 1;
            }
        })
    }

    /// Send packet and return packet echoed back from adapter
    pub fn send(&mut self, packet: &mut J1939Packet) -> Result<J1939Packet> {
        let p = packet.clone();
        p.time();
        self.bus.push(p.clone());
        Ok(p)
    }
}

impl Drop for Rp1210 {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}
