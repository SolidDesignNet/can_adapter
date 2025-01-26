use std::{
    collections::VecDeque,
    io::{BufRead, BufReader},
    sync::{atomic::AtomicBool, Arc, Mutex},
    thread,
    time::Duration,
};

use anyhow::{Error, Result};
use serialport::{SerialPort, SerialPortInfo};

use crate::{
    bus::{Bus, PushBus},
    connection::{Connection, ConnectionFactory, DeviceDescriptor, ProtocolDescriptor},
    packet::J1939Packet,
};

type Speed = u32;
pub const CAN_SPEEDS: [Speed; 9] = [10, 20, 50, 100, 125, 250, 500, 800, 1000];

pub struct Slcan {
    bus: Box<dyn Bus<J1939Packet>>,
    outbound: Arc<Mutex<VecDeque<String>>>,
    running: Arc<AtomicBool>,
}

impl Slcan {
    pub fn new(port: &str, speed: u32) -> Result<Slcan> {
        let mut port = serialport::new(port, 1_000_000)
            .timeout(Duration::from_millis(1000))
            .open()?;

        let outbound: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
        let mut bus: PushBus<J1939Packet> = PushBus::new();
        let running = Arc::new(AtomicBool::new(true));

        // read all packets
        {
            let mut bus = bus.clone();
            let running = running.clone();
            let mut port = port.try_clone()?;
            thread::spawn(move || {
                eprintln!("about to loop");
                let mut buf = [0; 64];
                while running.load(std::sync::atomic::Ordering::Relaxed) {
                    match port.read(&mut buf) {
                        Ok(len) => {
                            let str = String::from_utf8_lossy(&buf[..len]);
                            bus.push(parse(&str));
                        }
                        Err(len) => eprintln!("err {len:?}"),
                    }
                }
            });
        }

        send_cmd(&mut port, b"C")?;
        send_cmd(&mut port, b"V")?;

        let speed_command = &format!("S{}", CAN_SPEEDS.binary_search(&speed).unwrap());
        send_cmd(&mut port, &speed_command.as_bytes())?;
        send_cmd(&mut port, b"O")?;

        // write outbound packets
        // {
        //     let outbound = outbound.clone();
        //     let running = running.clone();
        //     let mut port = port.try_clone()?;
        //     thread::spawn(move || {
        //         let one_milli = Duration::from_millis(1);
        //         while running.load(std::sync::atomic::Ordering::Relaxed) {
        //             // tx
        //             let packet = outbound.lock().unwrap().pop_front();
        //             match packet {
        //                 Some(p) => {
        //                     port.write_all(&p.as_bytes());
        //                 }
        //                 None => {
        //                     thread::sleep(one_milli);
        //                 }
        //             }
        //         }
        //     });
        // }

        Ok(Slcan {
            bus: Box::new(bus),
            outbound,
            running,
        })
    }
}

fn send_cmd(port: &mut Box<dyn SerialPort>, cmd: &[u8]) -> Result<()> {
    let str = String::from_utf8_lossy(cmd);
    eprintln!("command {str}");
    port.write_all(cmd)?;
    port.write_all(b"\r")?;
    port.flush()?;
    thread::sleep(Duration::from_millis(500));
    Ok(())
}

impl Drop for Slcan {
    fn drop(&mut self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}
fn parse(buf: &str) -> Option<J1939Packet> {
    parse_result(buf).ok()
}
const SIZE: usize = 8;
// 0CF00A00 8 FF FF 00 FE FF FF 00 00
fn parse_result(buf: &str) -> Result<J1939Packet> {
    let buf = buf.trim();

    let len = buf.len();
    if len < SIZE || len % 2 != 1 {
        let message = format!("Invalid buf {buf} len:{len} {}", len % 2);
       // eprintln!("{}",message);
        return Err(Error::msg(message));
    }
    let id = u32::from_str_radix(&buf[0..SIZE], 16)?;
    let payload: Result<Vec<u8>, _> = ((1+SIZE)..len)
        .step_by(2)
        .map(|i| u8::from_str_radix(&buf[i..i + 2], 16))
        .collect();
    Ok(J1939Packet::new(None, 1, id, &payload?))
}
fn unparse(buf: &J1939Packet) -> String {
    todo!()
}
impl Connection for Slcan {
    fn send(
        &mut self,
        packet: &crate::packet::J1939Packet,
    ) -> anyhow::Result<crate::packet::J1939Packet> {
        let mut q = self.outbound.lock().unwrap();
        q.push_back(unparse(packet));
        Ok(todo!())
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
