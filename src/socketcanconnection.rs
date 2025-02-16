use anyhow::Context;
use color_print::cformat;
use socketcan::enumerate;
use socketcan::CanFrame;
use socketcan::Frame;
use socketcan::Socket;

use socketcan::CanSocket;
use socketcan::SocketOptions;
use std::io::Write;
use std::option::Option;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::time::SystemTime;

use crate::connection::Connection;
use crate::connection::ConnectionFactory;
use crate::connection::DeviceDescriptor;
use crate::connection::ProtocolDescriptor;
use crate::packet::J1939Packet;
use crate::pushbus::PushBus;

/// ```sh
///   ip link set can0 up
///   ip link set can0 type can bitrate 500000
/// ```
///
/// PEAK:
/// ```
/// sudo bash -xc 'rmmod peak_usb && modprobe peak_usb && ipslink set can0 name peak && ip link set peak type can bitrate 500000 && ip link set peak up'
/// ```
#[derive(Clone)]
pub struct SocketCanConnection {
    socket: Arc<Mutex<CanSocket>>,
    bus: PushBus<J1939Packet>,
    running: Arc<AtomicBool>,
    start: SystemTime,
}

impl SocketCanConnection {
    // FIXME add speed support.  Currently requires root access to configure network stack!
    pub fn new(str: &str, speed: u64) -> Result<SocketCanConnection, anyhow::Error> {
        let socket_can_connection = SocketCanConnection {
            socket: Arc::new(Mutex::new(CanSocket::open(str)?)),
            bus: PushBus::new(),
            running: Arc::new(AtomicBool::new(false)),
            start: SystemTime::now(),
        };

        let mut scc = socket_can_connection.clone();
        {
            let can_socket = scc.socket.lock().unwrap();
            can_socket.set_loopback(true)?;
            can_socket.set_recv_own_msgs(true)?;
            can_socket.set_nonblocking(false)?;
            can_socket.set_read_timeout(Duration::from_millis(50))?;
            can_socket.set_write_timeout(Duration::from_millis(500))?;
        }
        thread::spawn(move || scc.run());
        Ok(socket_can_connection)
    }
    fn run(&mut self) {
        self.running.store(true, Ordering::Relaxed);
        while self.running.load(Ordering::Relaxed) {
            let read_raw_frame = self.socket.lock().unwrap().read_raw_frame();
            let p = if read_raw_frame.is_ok() {
                let frame = read_raw_frame.unwrap();
                let len = frame.can_dlc as usize;
                if 0xFFFF & (frame.can_id >> 8) == 0xFEEC {
                    eprintln!("{:X} {:X?}", frame.can_id, frame.data)
                }
                Some(J1939Packet::new_socketcan(
                    self.now(),
                    false,
                    frame.can_id & 0x7FFFFFFF,
                    &frame.data[..len],
                ))
            } else {
                const ONE_MILLI: Duration = Duration::from_millis(1);
                std::thread::sleep(ONE_MILLI);
                None
            };
            self.bus.push(p);
        }
    }
    fn now(&self) -> u32 {
        SystemTime::now()
            .duration_since(self.start)
            .expect("Time went backwards")
            .as_millis() as u32
    }
}

impl Connection for SocketCanConnection {
    fn send(&mut self, packet: &J1939Packet) -> Result<J1939Packet, anyhow::Error> {
        // listen for echo
        let mut i = self.iter_for(Duration::from_millis(1000));

        // send packet
        {
            let frame = CanFrame::from_raw_id(packet.id(), packet.data()).expect("Invalid data packet");
            let mut can_socket = self.socket.lock().unwrap();
            can_socket.write_frame(&frame)?;
            can_socket.flush()?;
        }
        self.bus.push(Some(J1939Packet::new_socketcan(
            self.now(),
            true,
            packet.id(),
            packet.data(),
        )));

        i.find(
            move |p| p.id() == packet.id(), /*&& p.data() == packet.data()*/
        )
        .context("no echo")
    }

    fn iter(&self) -> Box<dyn Iterator<Item = Option<J1939Packet>> + Send + Sync> {
        self.bus.iter()
    }
}
impl Drop for SocketCanConnection {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        self.bus.close();
        //let _ = self.thread.take().unwrap().join();
    }
}

struct SocketCanConnectionFactory {
    name: String,
    speed: u64,
}
impl ConnectionFactory for SocketCanConnectionFactory {
    fn new(&self) -> anyhow::Result<Box<dyn Connection>> {
        Ok(Box::new(SocketCanConnection::new(&self.name, self.speed)?) as Box<dyn Connection>)
    }

    fn command_line(&self) -> String {
        color_print::cformat!("socketcan {}", self.name)
    }

    fn name(&self) -> String {
        cformat!("Linux socketcan on {}", self.name).to_string()
    }
}
pub(crate) fn list_all() -> Result<ProtocolDescriptor, anyhow::Error> {
    Ok(ProtocolDescriptor {
        name: "socketcan".into(),
        instructions_url: "https://github.com/SolidDesignNet/j1939logger/blob/main/README.md"
            .to_string(),
        devices: enumerate::available_interfaces()?
            .iter()
            .map(|v| DeviceDescriptor {
                name: v.clone(),
                connections: vec![Box::new(SocketCanConnectionFactory {
                    name: v.clone(),
                    speed: 500000,
                }) as Box<dyn ConnectionFactory>],
            })
            .collect(),
    })
}
