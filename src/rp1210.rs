use std::{
    fmt::Display,
    sync::{Arc, LazyLock, RwLock},
};

use crate::connection::ConnectionFactory;
use crate::packet::*;
use crate::{connection::*, j1939::j1939_packet::*, pushbus::*};
use anyhow::*;
use libloading::os::windows::Symbol as WinSymbol;
use libloading::*;
use std::ffi::CString;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::*;
use std::time::Duration;
use std::time::Instant;

pub const PACKET_SIZE: usize = 1600;

type ClientConnectType = unsafe extern "stdcall" fn(i32, i16, *const char, i32, i32, i16) -> i16;
type SendType = unsafe extern "stdcall" fn(i16, *const u8, i16, i16, i16) -> i16;
type ReadType = unsafe extern "stdcall" fn(i16, *const u8, i16, i16) -> i16;
type CommandType = unsafe extern "stdcall" fn(u16, i16, *const u8, u16) -> i16;
type VersionType =
    unsafe extern "stdcall" fn(*const char, *const char, *const char, *const char) -> i16;
type ReadDetailedVersionType =
    unsafe extern "stdcall" fn(i16, *const char, *const char, *const char) -> i16;
type GetErrorType = unsafe extern "stdcall" fn(i16, *const u8) -> i16;
type ClientDisconnectType = unsafe extern "stdcall" fn(i16) -> i16;

pub struct Rp1210 {
    api: API,
    bus: PushBus<Packet>,
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
    version_fn: WinSymbol<VersionType>,
    read_detailed_version_fn: WinSymbol<ReadDetailedVersionType>,
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
            let version: Symbol<VersionType> = lib.get(b"RP1210_ReadVersion\0").unwrap();
            let detailed_version: Symbol<ReadDetailedVersionType> =
                lib.get(b"RP1210_ReadDetailedVersion\0").unwrap();
            API {
                id: 0,
                client_connect_fn: client_connect.into_raw(),
                send_fn: send.into_raw(),
                read_fn: read.into_raw(),
                send_command_fn: send_command.into_raw(),
                get_error_fn: get_error.into_raw(),
                disconnect_fn: disconnect.into_raw(),
                version_fn: version.into_raw(),
                read_detailed_version_fn: detailed_version.into_raw(),
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
    fn client_connect(&mut self, dev_id: i16, address: u8) -> Result<()> {
        let str = CONNECTION_STRING.read().unwrap().clone();
        let connection_string: &str = &str;
        let app_packetize: bool = *APP_PACKETIZATION.read().unwrap();
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

    fn send(&self, packet: &Packet) -> Result<i16> {
        let packet: J1939Packet = packet.into();
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
    }
}

#[allow(dead_code)]
impl Rp1210 {
    pub fn new(id: &str, device: i16, address: u8) -> Result<Rp1210> {
        let time_stamp_weight = time_stamp_weight(id)?;

        let mut api = API::new(id)?;
        let read = *api.read_fn;
        let get_error_fn = *api.get_error_fn;

        // there may be
        api.client_connect(device, address)?;

        let id = api.id;

        let running = Arc::new(AtomicBool::new(true));
        let mut bus = PushBus::new("rp1210");
        let rp1210 = Rp1210 {
            api,
            bus: bus.clone(),
            running: running.clone(),
        };
        eprintln!(
            "RP1210 connected: {} device {} address {:02X}",
            id, device, address,
        );
        eprintln!("RP1210 version: {}", rp1210.version()?);
        eprintln!("RP1210 detailed: {}", rp1210.detailed_version()?);

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
                    let time = Duration::from_secs_f64(time as f64 * time_stamp_weight);
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
                    )
                    .into();
                    bus.push(Some(p));
                } else {
                    if size < 0 {
                        // read error
                        let code = -size;
                        let size = unsafe { (get_error_fn)(code, buf.as_mut_ptr()) } as usize;
                        let msg = String::from_utf8_lossy(&buf[0..size]).to_string();
                        let driver =
                            format!("{} {} {}", id, device, CONNECTION_STRING.read().unwrap());
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
    fn detailed_version(&self) -> Result<String> {
        let mut buf1: [u8; 256] = [0; 256];
        let mut buf2: [u8; 256] = [0; 256];
        let mut buf3: [u8; 256] = [0; 256];
        unsafe {
            (self.api.read_detailed_version_fn)(
                self.api.id,
                buf1.as_mut_ptr() as *const char,
                buf2.as_mut_ptr() as *const char,
                buf3.as_mut_ptr() as *const char,
            )
        };
        Ok(format!(
            "{} {} {}",
            String::from_utf8_lossy(&buf1),
            String::from_utf8_lossy(&buf2),
            String::from_utf8_lossy(&buf3)
        ))
    }
    fn version(&self) -> Result<String> {
        let mut buf1: [u8; 256] = [0; 256];
        let mut buf2: [u8; 256] = [0; 256];
        let mut buf3: [u8; 256] = [0; 256];
        let mut buf4: [u8; 256] = [0; 256];
        unsafe {
            (self.api.version_fn)(
                buf1.as_mut_ptr() as *const char,
                buf2.as_mut_ptr() as *const char,
                buf3.as_mut_ptr() as *const char,
                buf4.as_mut_ptr() as *const char,
            )
        };
        Ok(format!(
            "{} {} {} {}",
            String::from_utf8_lossy(&buf1),
            String::from_utf8_lossy(&buf2),
            String::from_utf8_lossy(&buf3),
            String::from_utf8_lossy(&buf4)
        ))
    }
}

impl Connection for Rp1210 {
    /// Send packet and return packet echoed back from adapter
    fn send(&self, packet: &Packet) -> Result<Packet> {
        let stream = self.bus.iter(); //_for();
        let sent = self.api.send(packet);
        // FIXME needs better error handling
        const DURATION: Duration = Duration::from_millis(50);
        let start = Instant::now();
        sent.map(|_| {
            stream
                .take_while(|_| Instant::now().duration_since(start) < DURATION)
                .flat_map(|o| o)
                .find(move |p| p.id == packet.id && p.payload == packet.payload)
                .expect("Echo failed.")
        })
    }

    fn iter(&self) -> Box<dyn Iterator<Item = Option<Packet>> + Send + Sync> {
        self.bus.iter()
    }
}

pub static CONNECTION_STRING: LazyLock<Arc<RwLock<String>>> =
    LazyLock::new(|| Arc::new(RwLock::new("J1939".into())));
pub static APP_PACKETIZATION: LazyLock<Arc<RwLock<bool>>> =
    LazyLock::new(|| Arc::new(RwLock::new(false)));

#[derive(Debug)]
struct Rp1210Device {
    pub id: i16,
    pub name: String,
    pub description: String,
}
#[derive(Debug)]
struct Rp1210Product {
    pub id: String,
    pub description: String,
    pub devices: Vec<Rp1210Device>,
}

struct Rp1210Factory {
    id: String,
    device: i16,
    address: u8,
    name: String,
}
impl Display for Rp1210Factory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
impl ConnectionFactory for Rp1210Factory {
    // FIXME should be impl From<Rp1210Factory> for Rp1210
    fn create(&self) -> Result<Box<dyn crate::connection::Connection>, anyhow::Error> {
        Ok(Box::new(Rp1210::new(&self.id, self.device, self.address)?) as Box<dyn Connection>)
    }

    fn command_line(&self) -> String {
        color_print::cformat!("rp1210 {} {}", self.id, self.device)
    }

    fn name(&self) -> String {
        self.name.to_string()
    }
}

impl Display for Rp1210Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}:{}", self.id, self.name, self.description)
    }
}
impl Display for Rp1210Product {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} {}", self.id, self.description)?;
        for d in &self.devices {
            writeln!(f, "{}", d)?;
        }
        std::fmt::Result::Ok(())
    }
}

/// legacy.  Should be inlined into list_all
fn list_all_products() -> Result<Vec<Rp1210Product>> {
    let start = std::time::Instant::now();
    let filename = "c:\\Windows\\RP121032.ini";
    let load_from_file = ini::Ini::load_from_file(filename);
    if load_from_file.is_err() {
        eprintln!(
            "Unable to process RP1210 file, {}.\n  {:?}",
            filename,
            load_from_file.err()
        );
        return Ok(vec![]);
    }
    let rtn = Ok(load_from_file?
        .get_from(Some("RP1210Support"), "APIImplementations")
        .unwrap_or("")
        .split(',')
        .map(|s| {
            let (description, devices) = list_devices_for_prod(s).unwrap_or_default();
            Rp1210Product {
                id: s.to_string(),
                description: description.to_string(),
                devices,
            }
        })
        .collect());
    println!("RP1210 INI parsing in {} ms", start.elapsed().as_millis());
    rtn
}

fn list_devices_for_prod(id: &str) -> Result<(String, Vec<Rp1210Device>)> {
    let start = std::time::Instant::now();
    let ini = ini::Ini::load_from_file(&format!("c:\\Windows\\{}.ini", id))?;

    // find device IDs for J1939
    let j1939_devices: Vec<&str> = ini
        .iter()
        // find J1939 protocol description
        .filter(|(section, properties)| {
            section.unwrap_or("").starts_with("ProtocolInformation")
                && properties.get("ProtocolString") == Some("J1939")
        })
        // which device ids support J1939?
        .flat_map(|(_, properties)| {
            properties
                .get("Devices")
                .map_or(vec![], |s| s.split(',').collect())
        })
        .collect();

    // find the specified devices
    let rtn = ini
        .iter()
        .filter(|(section, properties)| {
            section
                .map(|n| n.starts_with("DeviceInformation"))
                .unwrap_or(false)
                && properties
                    .get("DeviceID")
                    .map(|id| j1939_devices.contains(&id))
                    .unwrap_or(false)
        })
        .map(|(_, properties)| Rp1210Device {
            id: properties
                .get("DeviceID")
                .unwrap_or("0")
                .parse()
                .unwrap_or(-1),
            name: properties
                .get("DeviceName")
                .unwrap_or("Unknown")
                .to_string(),
            description: properties
                .get("DeviceDescription")
                .unwrap_or("Unknown")
                .to_string(),
        })
        .collect();
    println!("  {}.ini parsing in {} ms", id, start.elapsed().as_millis());
    let description = ini
        .section(Some("VendorInformation"))
        .and_then(|s| s.get("Name"))
        .unwrap_or_default()
        .to_string();
    Ok((description, rtn))
}

pub fn time_stamp_weight(id: &str) -> Result<f64> {
    let ini = ini::Ini::load_from_file(&format!("c:\\Windows\\{}.ini", id))?;
    Ok(ini
        .get_from_or::<&str>(Some("VendorInformation"), "TimeStampWeight", "1")
        .parse()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple() -> Result<()> {
        list_all_products()?;
        Ok(())
    }
}

pub fn list_all() -> Result<ProtocolDescriptor, anyhow::Error> {
    Ok(ProtocolDescriptor {
        name: "RP1210".into(),
        instructions_url: "http://fixme".to_string(),
        devices: list_all_products()?
            .iter()
            .map(|p| DeviceDescriptor {
                name: p.description.clone(),
                connections: p
                    .devices
                    .iter()
                    .map(|d| {
                        Box::new(Rp1210Factory {
                            id: p.id.clone(),
                            device: d.id,
                            address: 0xF9,
                            name: d.description.clone(),
                        }) as Box<dyn ConnectionFactory>
                    })
                    .collect(),
            })
            .collect(),
    })
}
