use crate::multiqueue::*;
use crate::packet::*;
use crate::rp1210_parsing;
use anyhow::*;
use libloading::os::windows::Symbol as WinSymbol;
use libloading::*;
use std::ffi::CString;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::*;
use std::sync::*;
use std::time::Duration;

pub const PACKET_SIZE: usize = 1600;

type ClientConnectType = unsafe extern "stdcall" fn(i32, i16, *const char, i32, i32, i16) -> i16;
type SendType = unsafe extern "stdcall" fn(i16, *const u8, i16, i16, i16) -> i16;
type ReadType = unsafe extern "stdcall" fn(i16, *const u8, i16, i16) -> i16;
type CommandType = unsafe extern "stdcall" fn(u16, i16, *const u8, u16) -> i16;
type _VERSION = unsafe extern "stdcall" fn(i16, *const u8, i16, i16) -> i16;
type GetErrorType = unsafe extern "stdcall" fn(i16, *const u8) -> i16;
type ClientDisconnectType = unsafe extern "stdcall" fn(i16) -> i16;

pub struct Rp1210 {
    pub bus: MultiQueue<J1939Packet>,
    api: API,
    time_stamp_weight: f64,
    pub running: Arc<AtomicBool>,
    pub id: String,
    pub device: i16,
    pub connection_string: String,
}
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
        let buf = &packet.packet.data;
        self.verify_return(unsafe { (self.send_fn)(self.id, buf.as_ptr(), buf.len() as i16, 0, 0) })
    }
}

impl Drop for Rp1210 {
    fn drop(&mut self) {
        self.running.store(false, Relaxed)
    }
}

#[allow(dead_code)]
impl Rp1210 {
    pub fn new(
        id: &str,
        device: i16,
        connection_string: &str,
        address: u8,
        bus: MultiQueue<J1939Packet>,
    ) -> Result<Rp1210> {
        let mut api = API::new(id)?;
        api.client_connect(device, connection_string, address, false)?;
        Ok(Rp1210 {
            api,
            bus,
            time_stamp_weight: rp1210_parsing::time_stamp_weight(id)?,
            running: Arc::new(AtomicBool::new(false)),
            id: id.to_string(),
            device,
            connection_string: connection_string.to_string(),
        })
    }
    /// background thread to read all packets into queue
    pub fn run(&mut self) {
        let read = *self.api.read_fn;
        let get_error_fn = *self.api.get_error_fn;
        let running = self.running.clone();
        let id = self.api.id;
        let mut bus = self.bus.clone();
        let time_stamp_weight = self.time_stamp_weight;
        running.store(true, Relaxed);
        let driver = format!("{} {} {}", self.id, self.device, self.connection_string);
        std::thread::spawn(move || {
            let mut buf: [u8; PACKET_SIZE] = [0; PACKET_SIZE];
            while running.load(Relaxed) {
                let size = unsafe { read(id, buf.as_mut_ptr(), PACKET_SIZE as i16, 0) };
                if size > 0 {
                    bus.push(J1939Packet::new_rp1210(
                        &buf[0..size as usize],
                        time_stamp_weight,
                    ))
                } else {
                    if size < 0 {
                        // read error
                        let code = -size;
                        let size = unsafe { (get_error_fn)(code, buf.as_mut_ptr()) } as usize;
                        let msg = String::from_utf8_lossy(&buf[0..size]).to_string();
                        eprintln!("ERROR: {}: {}: {}", driver, code, msg,);
                        std::thread::sleep(Duration::from_secs_f32(0.25))
                    }
                    std::thread::yield_now();
                }
            }
        });
    }

    /// Send packet and return packet echoed back from adapter
    pub fn send(&self, packet: &J1939Packet) -> Result<J1939Packet> {
        let mut stream = self.bus.iter_for(Duration::from_secs(2));
        let send = self.api.send(packet);
        // FIXME needs better error handling
        send.map(|_| stream.find(move |p| p.data() == packet.data()).unwrap())
    }

    pub fn close(&self) {
        self.running.store(false, Relaxed)
    }
}
