use anyhow::*;
use std::cell::RefCell;
use std::sync::atomic::*;
use std::sync::*;
use std::thread::Builder;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::connection::{Connection, ConnectionFactory, DeviceDescriptor, ProtocolDescriptor};
use crate::packet::*;
use crate::pushbus::PushBus;

#[derive(Clone)]
pub struct SimulatedConnection {
    bus: Box<PushBus<J1939Packet>>,
    running: Arc<AtomicBool>,
}
impl SimulatedConnection {
    pub fn new() -> Result<SimulatedConnection> {
        let bus = PushBus::new("sim connextion");
        let running = Arc::new(AtomicBool::new(false));
        {
            let running = running.clone();
            let bus = bus.clone();
            Builder::new()
                .name("simulated connection".into())
                .spawn(move || run(running, bus))?;
        }
        Ok(SimulatedConnection {
            bus: Box::new(bus.clone()),
            running: running.clone(),
        })
    }
}

fn run(running: Arc<AtomicBool>, mut bus: PushBus<J1939Packet>) {
    running.store(true, Ordering::Relaxed);
    let mut seq: u64 = u64::from_be_bytes([0, 0, 0, 0, 0, 0, 0, 0]);
    while running.load(Ordering::Relaxed) {
        let packet = J1939Packet::new_packet(Some(now()), 0, 6, 0xFEF1, 0, 0x0, &seq.to_be_bytes());
        bus.push(Some(packet));
        std::thread::sleep(Duration::from_millis(100));
        seq += 1;
    }
}

impl Connection for SimulatedConnection {
    /// Send packet and return packet echoed back from adapter
    fn send(&self, packet: &J1939Packet) -> Result<J1939Packet> {
        let packet = J1939Packet::new_packet(
            Some(now()),
            packet.channel(),
            packet.priority(),
            packet.pgn(),
            packet.dest(),
            packet.source(),
            packet.data(),
        );
        self.bus.push(Some(packet.clone()));
        Ok(packet)
    }

    fn iter(&self) -> Box<dyn Iterator<Item = Option<J1939Packet>> + Send + Sync> {
        self.bus.iter()
    }
}

fn now() -> u32 {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis();
    since_the_epoch as u32
}

impl Drop for SimulatedConnection {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}
struct SimulatedConnectionFactory {}
impl ConnectionFactory for SimulatedConnectionFactory {
    fn create(&self) -> Result<Box<dyn Connection>> {
        Ok(Box::new(SimulatedConnection::new()?) as Box<dyn Connection>)
    }

    fn command_line(&self) -> String {
        "sim".to_string()
    }

    fn name(&self) -> String {
        "Simulated CAN stream".to_string()
    }
}
pub fn factory() -> Result<ProtocolDescriptor, anyhow::Error> {
    Ok(ProtocolDescriptor {
        name: "Simulation".to_string(),
        instructions_url: "https://github.com/SolidDesignNet/j1939logger/blob/main/README.md"
            .to_string(),
        devices: vec![DeviceDescriptor {
            name: "one".to_string(),
            connections: vec![Box::new(SimulatedConnectionFactory {})],
        }],
    })
}
