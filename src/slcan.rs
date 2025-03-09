use std::{
    collections::VecDeque,
    sync::{atomic::AtomicBool, Arc, Mutex},
    thread,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Error, Result};
use serialport::{SerialPort, SerialPortInfo};

use crate::{
    connection::{Connection, ConnectionFactory, DeviceDescriptor, ProtocolDescriptor},
    packet::J1939Packet,
    pushbus::PushBus,
};

type Speed = u32;
pub const CAN_SPEEDS: [Speed; 9] = [10, 20, 50, 100, 125, 250, 500, 800, 1000];

#[derive(Clone)]
pub struct Slcan {
    bus: PushBus<J1939Packet>,
    outbound: Arc<Mutex<VecDeque<String>>>,
    running: Arc<AtomicBool>,
    start: SystemTime,
}

const ONE_MILLI: Duration = Duration::from_millis(1);

impl Slcan {
    pub fn new(port_name: &str, speed: u32) -> Result<Slcan> {
        eprintln!("opening {port_name}");
        let mut port = serialport::new(port_name, 1_000_000)
            .timeout(ONE_MILLI)
            .flow_control(serialport::FlowControl::Hardware)
            .dtr_on_open(true)
            .open()?;

        port.clear(serialport::ClearBuffer::All)?;

        let slcan = Slcan {
            bus: PushBus::new(),
            outbound: Arc::new(Mutex::new(VecDeque::new())),
            running: Arc::new(AtomicBool::new(true)),
            start: SystemTime::now(),
        };

        send_cmd(&mut port, b"C")?;
        send_cmd(&mut port, b"C")?;
        send_cmd(&mut port, b"V")?;
        let speed_command = &format!("S{}", CAN_SPEEDS.binary_search(&speed).unwrap());
        send_cmd(&mut port, &speed_command.as_bytes())?;
        send_cmd(&mut port, b"O")?;

        // write outbound packets
        {
            let mut slcan = slcan.clone();
            thread::spawn(move || slcan.run_can(port));
        }

        eprintln!(" opened {port_name}");
        Ok(slcan)
    }
    pub fn now(&self) -> u32 {
        SystemTime::now()
            .duration_since(self.start)
            .expect("Time went backwards")
            .as_millis() as u32
    }

    fn run_can(&mut self, mut port: Box<dyn SerialPort>) {
        // gross
        // copy from port to buf
        // copy from buf vecdeque
        // copy line from vecdeque to vec
        // parse string into byte[]

        let mut buf = [0; 1024];
        let mut q = VecDeque::new();

        while self.running.load(std::sync::atomic::Ordering::Relaxed) {
            // tx
            {
                let mut items = self.outbound.lock().unwrap();

                let packet = items.pop_front();
                match packet {
                    Some(p) => {
                        port.write_all(&p.as_bytes())
                            .expect("Unable to write to serialport.");
                        port.write_all(b"\r")
                            .expect("Unable to write CR to serialport.");
                        port.flush().expect("Unable to flush serialport.");
                    }
                    None => {}
                }
            }
            // rx
            // not spinning, because port.read() is blocking
            match port.read(&mut buf) {
                Ok(len) => {
                    if len > 0 {
                        q.extend(buf[..len].into_iter());
                        loop {
                            let index = q.iter().take_while(|u| **u != b'\r').count();
                            if index < q.len() {
                                let vec: Vec<u8> = q.drain(..index).collect();
                                let line: String = String::from_utf8(vec).expect("Invalid UTF8");
                                self.bus.push(parse_result(self.now(), line).ok());
                                q.pop_front(); // drop \r
                            } else {
                                break;
                                // no \r
                            }
                        }
                    }
                }
                Err(_error) => {
                    //   eprintln!("{_error}");
                }
            }
        }
    }
}

fn send_cmd(port: &mut Box<dyn SerialPort>, cmd: &[u8]) -> Result<()> {
    eprintln!("sending cmd {}", String::from_utf8(cmd.into())?);
    port.write_all(cmd)?;
    port.write_all(b"\r")?;
    port.flush()?;
    Ok(())
}

impl Drop for Slcan {
    fn drop(&mut self) {
        if self.running.load(std::sync::atomic::Ordering::Relaxed) {
            self.running
                .store(false, std::sync::atomic::Ordering::Relaxed);
            self.bus.close();
            // give Windows time to clean up device
            //            thread::sleep(TIMEOUT * 2);
        }
    }
}

const SIZE: usize = 9;
// 0CF00A00 8 FF FF 00 FE FF FF 00 00
fn parse_result(now: u32, buf: String) -> Result<J1939Packet> {
    let len = buf.len();
    // {T}{4 * 2 digit hex bytes}{1 digit length}{2 digit hex payload}
    if len < SIZE || len % 2 != 0 {
        let message = format!("Invalid buf [{buf}] len:{len} {}", len % 2);
        eprintln!("{}", message);
        return Err(Error::msg(message));
    }
    let id = u32::from_str_radix(&buf[1..SIZE], 16)?;
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
        #[cfg(slcan_echo)]
        let mut echo_stream = self.iter_for(Duration::from_millis(2_000));

        // send packet
        let p = unparse(packet);
        {
            let mut items = self.outbound.lock().unwrap();
            let len = items.len();
            if len > 200 {
                eprintln!("queue too deep: {len}");
            }
            items.push_back(p);
        }

        // return echo
        #[cfg(slcan_echo)]
        let r = echo_stream
            .find(
                move |p| p.id() == packet.id(), /*&& p.data() == packet.data()*/
            )
            .context("no echo");
        #[cfg(not(slcan_echo))]
        let r = {
            self.bus.push(Some(packet.clone()));
            Ok(packet.clone())
        };
        r
    }

    fn iter(&self) -> Box<dyn Iterator<Item = Option<crate::packet::J1939Packet>> + Send + Sync> {
        self.bus.iter()
    }
}
struct SclanFactory {
    port_info: SerialPortInfo,
    speed: u32,
}

impl ConnectionFactory for SclanFactory {
    fn new(&self) -> Result<Box<dyn Connection>> {
        Slcan::new(self.port_info.port_name.as_str(), self.speed)
            .map(|c| Box::new(c) as Box<dyn Connection>)
    }

    fn command_line(&self) -> String {
        color_print::cformat!("slcan {} {}", self.port_info.port_name, self.speed)
    }

    fn name(&self) -> String {
        format!("SLCAN {}", self.speed)
    }
}

pub fn list_all() -> Result<ProtocolDescriptor> {
    let devices = serialport::available_ports()?
        .into_iter()
        .map(|port_info| {
            let connections = CAN_SPEEDS
                .into_iter()
                .map(|speed| {
                    Box::new(SclanFactory {
                        port_info: port_info.clone(),
                        speed,
                    }) as Box<dyn ConnectionFactory>
                })
                .collect();
            DeviceDescriptor {
                name: port_info.port_name.clone(),
                connections,
            }
        })
        .collect();
    Ok(ProtocolDescriptor {
        name: "SLCAN".to_string(),
        devices,
        instructions_url: "http://fixme".to_string(),
    })
}
