use std::{
    collections::VecDeque,
    sync::{atomic::AtomicBool, Arc, Mutex},
    thread,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Error, Result};
use serialport::{SerialPort, SerialPortInfo};

use crate::{
    bus::{Bus, PushBus},
    connection::{Connection, ConnectionFactory, DeviceDescriptor, ProtocolDescriptor},
    packet::J1939Packet,
};

type Speed = u32;
pub const CAN_SPEEDS: [Speed; 9] = [10, 20, 50, 100, 125, 250, 500, 800, 1000];

#[derive(Clone)]
pub struct Slcan {
    bus: Box<dyn Bus<J1939Packet>>,
    outbound: Arc<Mutex<VecDeque<String>>>,
    running: Arc<AtomicBool>,
    start: SystemTime,
}

impl Slcan {
    pub fn new(port: &str, speed: u32) -> Result<Slcan> {
        let mut port = serialport::new(port, 1_000_000)
            .timeout(Duration::from_millis(1000))
            .open()?;

        let slcan = Slcan {
            bus: Box::new(PushBus::new()),
            outbound: Arc::new(Mutex::new(VecDeque::new())),
            running: Arc::new(AtomicBool::new(true)),
            start: SystemTime::now(),
        };

        // read all packets
        {
            let mut slcan = slcan.clone();
            let port = port.try_clone()?;
            thread::spawn(move || slcan.from_can(port));
        }

        send_cmd(&mut port, b"C")?;
        let speed_command = &format!("S{}", CAN_SPEEDS.binary_search(&speed).unwrap());
        send_cmd(&mut port, &speed_command.as_bytes())?;
        send_cmd(&mut port, b"O")?;

        // write outbound packets
        {
            let slcan = slcan.clone();
            thread::spawn(move || slcan.to_can(port));
        }

        Ok(slcan)
    }
    pub fn now(&self) -> u32 {
        SystemTime::now()
            .duration_since(self.start)
            .expect("Time went backwards")
            .as_millis() as u32
    }

    fn to_can(&self, mut port: Box<dyn SerialPort>) {
        let one_milli = Duration::from_millis(1);
        while self.running.load(std::sync::atomic::Ordering::Relaxed) {
            // tx
            let packet = self.outbound.lock().unwrap().pop_front();
            match packet {
                Some(p) => {
                    eprintln!("sending: {p}");
                    port.write_all(&p.as_bytes())
                        .expect("Unable to write to serialport.");
                    port.write_all(b"\r")
                        .expect("Unable to write to serialport.");
                    port.flush();
                    eprintln!("sent");
                }
                None => {
                    thread::sleep(one_milli);
                }
            }
        }
    }

    fn from_can(&mut self, mut port: Box<dyn SerialPort>) {
        let mut buf = [0; 64];
        while self.running.load(std::sync::atomic::Ordering::Relaxed) {
            match port.read(&mut buf) {
                Ok(len) => {
                    let str = String::from_utf8_lossy(&buf[..len]);
                    self.bus.push(parse_result(self.now(), &str).ok());
                }
                Err(len) => eprintln!("err {len:?}"),
            }
        }
    }
}

fn send_cmd(port: &mut Box<dyn SerialPort>, cmd: &[u8]) -> Result<()> {
    port.write_all(cmd)?;
    port.write_all(b"\r")?;
    port.flush()?;
    Ok(())
}

impl Drop for Slcan {
    fn drop(&mut self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

const SIZE: usize = 8;
// 0CF00A00 8 FF FF 00 FE FF FF 00 00
fn parse_result(now: u32, buf: &str) -> Result<J1939Packet> {
    let buf = buf.trim();

    let len = buf.len();
    if len < SIZE || len % 2 != 1 {
        let message = format!("Invalid buf {buf} len:{len} {}", len % 2);
     eprintln!("{}",message);
        return Err(Error::msg(message));
    }
    let id = u32::from_str_radix(&buf[0..SIZE], 16)?;
    let payload: Result<Vec<u8>, _> = ((1 + SIZE)..len)
        .step_by(2)
        .map(|i| u8::from_str_radix(&buf[i..i + 2], 16))
        .collect();
    Ok(J1939Packet::new(Some(now), 1, id, &payload?))
}

fn unparse(p: &J1939Packet) -> String {
    let payload = p.data_str_nospace();
    format!("T{:08X}{}{}", p.id(), payload.len() / 2, payload)
}

impl Connection for Slcan {
    fn send(
        &mut self,
        packet: &crate::packet::J1939Packet,
    ) -> anyhow::Result<crate::packet::J1939Packet> {
        // listen for echo
        let mut i = self.iter_for(Duration::from_millis(2_000));

        // send packet
        {
            let p = unparse(packet);
            eprintln!("enqueuing: {p}");
            let mut q = self.outbound.lock().unwrap();
            q.push_back(p);
        }
        self.bus.push(Some(packet.clone()));
        // FIXME
        // i.find(
        //     move |p| p.id() == packet.id(), /*&& p.data() == packet.data()*/
        // )
        // .context("no echo")
        Ok(packet.clone())
    }

    fn iter(&self) -> Box<dyn Iterator<Item = Option<crate::packet::J1939Packet>> + Send + Sync> {
        self.bus.iter()
    }
}
struct SclanFactory {
    port: SerialPortInfo,
    speed: u32,
}

impl ConnectionFactory for SclanFactory {
    fn new(&self) -> Result<Box<dyn Connection>> {
        Slcan::new(self.port.port_name.as_str(), self.speed)
            .map(|c| Box::new(c) as Box<dyn Connection>)
    }

    fn command_line(&self) -> String {
        color_print::cformat!("slcan {} {}", self.port.port_name, self.speed)
    }

    fn name(&self) -> String {
        "SLCAN".to_string()
    }
}

pub fn list_all() -> Result<ProtocolDescriptor> {
    Ok(ProtocolDescriptor {
        name: "SLCAN".into(),
        devices: serialport::available_ports()?
            .into_iter()
            .map(|port| {
                let c = CAN_SPEEDS
                    .into_iter()
                    .map(|speed| {
                        Box::new(SclanFactory {
                            port: port.clone(),
                            speed,
                        }) as Box<dyn ConnectionFactory>
                    })
                    .collect();
                DeviceDescriptor {
                    name: port.port_name.clone(),
                    connections: c,
                }
            })
            .collect(),
    })
}
