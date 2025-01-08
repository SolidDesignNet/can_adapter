use crate::bus::*;
use crate::connection::Connection;
use crate::packet::*;
use crate::rp1210_parsing;
use anyhow::*;
#[cfg(not(windows))]
use libloading::os::unix::Symbol as WinSymbol;
#[cfg(windows)]
use libloading::os::windows::Symbol as WinSymbol;
use libloading::*;
use std::ffi::CString;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::*;
use std::sync::*;
use std::time::Duration;
use std::time::Instant;

pub const PACKET_SIZE: usize = 1600;

type ClientConnectType = unsafe extern "stdcall" fn(i32, i16, *const char, i32, i32, i16) -> i16;
type SendType = unsafe extern "stdcall" fn(i16, *const u8, i16, i16, i16) -> i16;
type ReadType = unsafe extern "stdcall" fn(i16, *const u8, i16, i16) -> i16;
type CommandType = unsafe extern "stdcall" fn(u16, i16, *const u8, u16) -> i16;
type _VERSION = unsafe extern "stdcall" fn(i16, *const u8, i16, i16) -> i16;
type GetErrorType = unsafe extern "stdcall" fn(i16, *const u8) -> i16;
type ClientDisconnectType = unsafe extern "stdcall" fn(i16) -> i16;

pub struct Rp1210 {
    api: API,
    bus: Box<PushBus<J1939Packet>>,
    running: Arc<AtomicBool>,
}
#[derive(Debug)]
struct API {
    id: i16,

    _lib: Library,
    client_connect_fn: WinSymbol<ClientConnectType>,
    send_fn: WinSymbol<SendType>,
    read_fn: WinSymbol<ReadType>,
    send_command_fn: WinSymbol<CommandType>,
    get_error_fn: WinSymbol<GetErrorType>,
    disconnect_fn: WinSymbol<ClientDisconnectType>,
}
impl Drop for API {
    fn drop(&mut self) {
        unsafe { (*self.disconnect_fn)(self.id) };
    }
}
impl API {
    fn new(id: &str) -> Result<API> {
        Ok(unsafe {
            let lib = Library::new(id.to_string())?;
            let client_connect: Symbol<ClientConnectType> =
                lib.get(b"RP1210_ClientConnect\0").unwrap();
            let send: Symbol<SendType> = lib.get(b"RP1210_SendMessage\0").unwrap();
            let send_command: Symbol<CommandType> = lib.get(b"RP1210_SendCommand\0").unwrap();
            let read: Symbol<ReadType> = lib.get(b"RP1210_ReadMessage\0").unwrap();
            let get_error: Symbol<GetErrorType> = lib.get(b"RP1210_GetErrorMsg\0").unwrap();
            let disconnect: Symbol<ClientDisconnectType> =
                lib.get(b"RP1210_ClientDisconnect\0").unwrap();
            API {
                id: 0,
                client_connect_fn: client_connect.into_raw(),
                send_fn: send.into_raw(),
                read_fn: read.into_raw(),
                send_command_fn: send_command.into_raw(),
                get_error_fn: get_error.into_raw(),
                disconnect_fn: disconnect.into_raw(),
                _lib: lib,
            }
        })
    }
    fn send_command(&self, cmd: u16, buf: Vec<u8>) -> Result<i16> {
        self.verify_return(unsafe {
            (self.send_command_fn)(cmd, self.id, buf.as_ptr(), buf.len() as u16)
        })
    }
    fn get_error(&self, code: i16) -> Result<String> {
        let mut buf: [u8; 1024] = [0; 1024];
        let size = unsafe { (self.get_error_fn)(code, buf.as_mut_ptr()) } as usize;
        Ok(String::from_utf8_lossy(&buf[0..size]).to_string())
    }
    fn verify_return(&self, v: i16) -> Result<i16> {
        if v < 0 || v > 127 {
            Err(anyhow!(format!("code: {} msg: {}", v, self.get_error(v)?)))
        } else {
            Ok(v)
        }
    }
    fn client_connect(
        &mut self,
        dev_id: i16,
        connection_string: &str,
        address: u8,
        app_packetize: bool,
    ) -> Result<()> {
        let c_to_print = CString::new(connection_string).expect("CString::new failed");
        self.id = self.verify_return(unsafe {
            (self.client_connect_fn)(
                0,
                dev_id,
                c_to_print.as_ptr() as *const char,
                0,
                0,
                if app_packetize { 1 } else { 0 },
            )
        })?;
        if !app_packetize {
            self.send_command(
                /*CMD_PROTECT_J1939_ADDRESS*/ 19,
                vec![
                    address, 0, 0, 0xE0, 0xFF, 0, 0x81, 0, 0, /*CLAIM_BLOCK_UNTIL_DONE*/ 0,
                ],
            )?;
        }
        self.send_command(
            /*CMD_ECHO_TRANSMITTED_MESSAGES*/ 16,
            vec![/*ECHO_ON*/ 1],
        )?;
        self.send_command(/*CMD_SET_ALL_FILTERS_STATES_TO_PASS*/ 3, vec![])?;
        Ok(())
    }
    fn send(&self, packet: &J1939Packet) -> Result<i16> {
        let id = packet.pgn();
        let pgn = id.to_le_bytes();
        let buf = [
            &[
                pgn[0],
                pgn[1],
                pgn[2],
                packet.priority(),
                packet.source(),
                if id < 0xF000 { packet.dest() } else { 0 },
            ],
            packet.data(),
        ]
        .concat();
        self.verify_return(unsafe { (self.send_fn)(self.id, buf.as_ptr(), buf.len() as i16, 0, 0) })
    }
}

impl Drop for Rp1210 {
    fn drop(&mut self) {
        self.running.store(false, Relaxed);
        self.bus.close();
    }
}

#[allow(dead_code)]
impl Rp1210 {
    pub fn new(
        id: &str,
        device: i16,
        connection_string: &str,
        address: u8,
        app_packetize: bool,
    ) -> Result<Rp1210> {
        let time_stamp_weight = rp1210_parsing::time_stamp_weight(id)?;

        let mut api = API::new(id)?;
        let read = *api.read_fn;
        let get_error_fn = *api.get_error_fn;
        api.client_connect(device, connection_string, address, app_packetize)?;
        let id = api.id;

        let running = Arc::new(AtomicBool::new(true));
        let mut bus = PushBus::new();
        let rp1210 = Rp1210 {
            api,
            bus: Box::new(bus.clone()),
            running: running.clone(),
        };

        let connection_string = connection_string.to_string();
        std::thread::spawn(move || {
            let mut buf: [u8; PACKET_SIZE] = [0; PACKET_SIZE];
            let channel = 0; // FIXME channel.unwrap_or(0);
            while running.load(Relaxed) {
                let size = unsafe { read(id, buf.as_mut_ptr(), PACKET_SIZE as i16, 0) };
                if size > 0 {
                    let data = &buf[0..size as usize];
                    let time = u32::from_be_bytes(
                        data[0..4].try_into().expect("unable to decode timestamp"),
                    );
                    let echoed = data[4];
                    let payload = &data[11..(data.len())];
                    let priority = data[8] & 0x07;
                    let pgn = u32::from_be_bytes([0, data[7], data[6], data[5]]);
                    let sa = data[9];
                    let da = if pgn < 0xF000 { data[10] } else { 0 };
                    let pgn = pgn | (da as u32);

                    let p = J1939Packet::new_packet(
                        Some(time),
                        channel,
                        priority,
                        pgn,
                        da,
                        sa,
                        payload,
                    );
                    bus.push(Some(p));
                } else {
                    if size < 0 {
                        // read error
                        let code = -size;
                        let size = unsafe { (get_error_fn)(code, buf.as_mut_ptr()) } as usize;
                        let msg = String::from_utf8_lossy(&buf[0..size]).to_string();
                        let driver = format!("{} {} {}", id, device, connection_string);
                        eprintln!("ERROR: {}: {}: {}", driver, code, msg,);
                        std::thread::sleep(Duration::from_millis(250));
                    } else {
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
                bus.push(None)
            }
        });
        Ok(rp1210)
    }
}

impl Connection for Rp1210 {
    /// Send packet and return packet echoed back from adapter
    fn send(&mut self, packet: &J1939Packet) -> Result<J1939Packet> {
        let end = Instant::now() + Duration::from_millis(50);
        // FIXMEiter_unti
        let stream = self.bus.iter().take_while(|_| Instant::now() < end);
        let sent = self.api.send(packet);
        // FIXME needs better error handling
        sent.map(|_| {
            stream
                .flat_map(|o| o)
                .find(move |p| p.header() == packet.header() && p.data() == packet.data())
                .expect("Echo failed.")
        })
    }

    fn iter(&self) -> Box<dyn Iterator<Item = Option<J1939Packet>> + Send + Sync> {
        self.bus.iter()
    }
}
