use std::{
    collections::VecDeque,
    sync::{atomic::AtomicBool, Arc, Mutex},
    thread,
    time::{Duration, SystemTime},
};

use anyhow::{Error, Result};
use serialport::{SerialPort, SerialPortInfo};

use crate::{
    connection::{Connection, ConnectionFactory, DeviceDescriptor, ProtocolDescriptor},
    packet::Packet,
    pushbus::PushBus,
};

type Speed = u32;
pub const CAN_SPEEDS: [Speed; 9] = [10, 20, 50, 100, 125, 250, 500, 800, 1000];

#[derive(Clone)]
pub struct Slcan {
    bus: PushBus<Packet>,
    outbound: Arc<Mutex<VecDeque<String>>>,
    running: Arc<AtomicBool>,
    start: SystemTime,
    verbose: bool,
    port: Arc<Mutex<Box<dyn SerialPort>>>,
}

const ONE_MILLI: Duration = Duration::from_millis(1);

impl Slcan {
    pub fn new(verbose: bool, port_name: &str, speed: u32) -> Result<Slcan> {
        if verbose {
            eprintln!("opening {port_name}");
        }
        let port = serialport::new(port_name, 1_000_000)
            .timeout(ONE_MILLI)
            .flow_control(serialport::FlowControl::Hardware)
            .dtr_on_open(true)
            .open()?;

        port.clear(serialport::ClearBuffer::All)?;

        let mut slcan = Slcan {
            bus: PushBus::new("slcan"),
            outbound: Arc::new(Mutex::new(VecDeque::new())),
            running: Arc::new(AtomicBool::new(true)),
            start: SystemTime::now(),
            verbose,
            port: Arc::new(Mutex::new(port)),
        };

        slcan.send_cmd(b"C")?;
        slcan.send_cmd(b"C")?;
        slcan.send_cmd(b"V")?;
        let speed_command = &format!("S{}", CAN_SPEEDS.binary_search(&speed).unwrap());
        slcan.send_cmd(speed_command.as_bytes())?;
        slcan.send_cmd(b"O")?;

        // write outbound packets
        {
            let mut slcan = slcan.clone();
            thread::spawn(move || slcan.run_can());
        }
        if verbose {
            eprintln!(" opened {port_name}");
        }
        Ok(slcan)
    }

    fn now(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.start)
            .expect("Time went backwards")
    }

    fn run_can(&mut self) {
        // gross
        // copy from port to buf
        // copy from buf vecdeque
        // copy line from vecdeque to vec
        // parse string into byte[]

        let mut buf = [0; 1024];
        let mut q = VecDeque::new();

        let mut port = self.port.lock().unwrap();
        while self.running.load(std::sync::atomic::Ordering::Relaxed) {
            // tx
            {
                let mut items = self.outbound.lock().unwrap();

                let packet = items.pop_front();
                if let Some(p) = packet {
                    port.write_all(p.as_bytes())
                        .expect("Unable to write to serialport.");
                    port.write_all(b"\r")
                        .expect("Unable to write CR to serialport.");
                    port.flush().expect("Unable to flush serialport.");
                }
            }
            // rx
            // not spinning, because port.read() is blocking
            match port.read(&mut buf) {
                Ok(len) => {
                    if len > 0 {
                        q.extend(buf[..len].iter());
                        loop {
                            let index = q.iter().take_while(|u| **u != b'\r').count();
                            if index < q.len() {
                                let vec: Vec<u8> = q.drain(..index).collect();
                                let line: String = String::from_utf8(vec).expect("Invalid UTF8");
                                self.bus.push(self.parse_result(line).ok());
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

    fn send_cmd(&mut self, cmd: &[u8]) -> Result<()> {
        if self.verbose {
            eprintln!("sending cmd {}", String::from_utf8(cmd.into())?);
        }
        let mut port = self.port.lock().unwrap();
        port.write_all(cmd)?;
        port.write_all(b"\r")?;
        port.flush()?;
        Ok(())
    }

    // 0CF00A00 8 FF FF 00 FE FF FF 00 00
    fn parse_result(&self, buf: String) -> Result<Packet> {
        const SIZE: usize = 9;
        let now = self.now();
        let len = buf.len();
        // {T}{4 * 2 digit hex bytes}{1 digit length}{2 digit hex payload}
        if len < SIZE || !len.is_multiple_of(2) {
            let message = format!("Invalid buf [{buf}] len:{len} {}", len % 2);
            if self.verbose {
                eprintln!("{message}");
            }
            return Err(Error::msg(message));
        }
        let id = u32::from_str_radix(&buf[1..SIZE], 16)?;
        let payload: Result<Vec<u8>, _> = ((1 + SIZE)..len)
            .step_by(2)
            .map(|i| u8::from_str_radix(&buf[i..i + 2], 16))
            .collect();
        Ok(Packet::new_rx(id, &payload?, now, 0))
    }
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

fn unparse(p: &Packet) -> String {
    let payload = p.payload_str_nospace();
    format!("T{:08X}{}{}", p.id, payload.len() / 2, payload)
}

impl Connection for Slcan {
    fn send(&self, packet: &Packet) -> anyhow::Result<Packet> {
        // send packet
        self.outbound.lock().unwrap().push_back(unparse(packet));

        // SLCAN does not support echo, so wait until outbound is empty;
        while !self.outbound.lock().unwrap().is_empty() {
            thread::sleep(ONE_MILLI);
        }

        self.bus.push(Some(packet.clone()));
        Ok(packet.clone())
    }

    fn iter(&self) -> Box<dyn Iterator<Item = Option<crate::packet::Packet>> + Send + Sync> {
        self.bus.iter()
    }
}
struct SclanFactory {
    port_info: SerialPortInfo,
    speed: u32,
}

impl ConnectionFactory for SclanFactory {
    fn create(&self) -> Result<Box<dyn Connection>> {
        Slcan::new(false, self.port_info.port_name.as_str(), self.speed)
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
