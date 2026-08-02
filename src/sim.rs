use anyhow::*;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::atomic::*;
use std::thread::Builder;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{iter, sync::*};

use crate::connection::{Connection, ConnectionFactory, DeviceDescriptor, ProtocolDescriptor};
use crate::j1939::j1939_packet::J1939Packet;
use crate::packet::*;
use crate::pushbus::PushBus;

#[derive(Clone)]
pub struct SimulatedConnection {
    bus: Box<PushBus<Packet>>,
    running: Arc<AtomicBool>,
}
impl SimulatedConnection {
    pub fn new(file: Option<String>) -> Result<SimulatedConnection> {
        let bus = PushBus::new("sim connextion");
        let running = Arc::new(AtomicBool::new(false));
        {
            let running = running.clone();
            let bus = bus.clone();
            Builder::new()
                .name("simulated connection".into())
                .spawn(move || {
                    let packets = if let Some(file) = &file {
                        let f = File::open(file)?;
                        let i = iter::repeat_with(move || {
                            BufReader::new(f.try_clone().expect("Unable to reread {file}"))
                        })
                        .flat_map(|reader| reader.lines())
                        .filter_map(|line| line.ok()?.parse::<J1939Packet>().ok());
                        Box::new(i) as Box<dyn Iterator<Item = J1939Packet>>
                    } else {
                        let i = (0u64..).map(|n| {
                            J1939Packet::new_packet(
                                Some(now()),
                                0,
                                6,
                                0xFEF1,
                                0,
                                0x0,
                                &u64::to_be_bytes(n),
                            )
                        });
                        Box::new(i) as Box<dyn Iterator<Item = J1939Packet>>
                    };
                    run(running, bus, packets)
                })?;
        }
        Ok(SimulatedConnection {
            bus: Box::new(bus.clone()),
            running: running.clone(),
        })
    }
}

fn run(
    running: Arc<AtomicBool>,
    bus: PushBus<Packet>,
    mut packets: impl Iterator<Item = J1939Packet>,
) -> Result<()> {
    running.store(true, Ordering::Relaxed);
    let mut last_time = Duration::MAX;
    while running.load(Ordering::Relaxed) {
        let packet = packets.next().unwrap();
        if let Some(time) = packet.time() {
            std::thread::sleep(time.saturating_sub(last_time));
            last_time = time;
        }
        bus.push(Some(packet.into()));
    }
    Ok(())
}

impl Connection for SimulatedConnection {
    /// Send packet and return packet echoed back from adapter
    fn send(&self, packet: &Packet) -> Result<Packet> {
        let j1939: J1939Packet = packet.into();
        let packet: Packet = J1939Packet::new_packet(
            Some(now()),
            j1939.channel().unwrap_or_default(),
            j1939.priority(),
            j1939.pgn(),
            j1939.dest(),
            j1939.source(),
            {
                let this = &j1939;
                &this.payload
            },
        )
        .into();
        self.bus.push(Some(packet.clone()));
        Ok(packet)
    }

    fn iter(&self) -> Box<dyn Iterator<Item = Option<Packet>> + Send + Sync> {
        self.bus.iter()
    }
}

fn now() -> Duration {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
}

impl Drop for SimulatedConnection {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}
struct SimulatedConnectionFactory {}
impl ConnectionFactory for SimulatedConnectionFactory {
    fn create(&self) -> Result<Box<dyn Connection>> {
        Ok(Box::new(SimulatedConnection::new(None)?) as Box<dyn Connection>)
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
