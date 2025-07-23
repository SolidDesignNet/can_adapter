use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;
use clap::*;
use clap_num::maybe_hex;
use connection::Connection;
use slcan::Slcan;

pub mod connection;
pub mod j1939;
pub mod j1939_packet;
pub mod packet;
pub mod pushbus;
pub mod sim;
pub mod slcan;
pub mod uds;

use j1939::J1939;
use uds::Uds;

#[cfg(windows)]
pub mod rp1210;
#[cfg(windows)]
use rp1210::Rp1210;

#[cfg(target_os = "linux")]
pub mod socketcanconnection;
#[cfg(target_os = "linux")]
use socketcanconnection::SocketCanConnection;

use crate::{j1939_packet::J1939Packet, packet::Packet};

#[derive(Parser)] // requires `derive` feature
#[command(name = "cancan")]
#[command(version,about = "CAN tool", long_about = None)]
pub struct CanCan {
    /// For a list of possible connections, "cancan list log".  Available connection strings will vary depending on the machine.
    pub connection: String,

    #[arg(long="sa", short('s'), default_value = "0xF9",value_parser=maybe_hex::<u8>)]
    /// Adapter Address (used for packets send and transport protocol)
    pub source_address: u8,

    #[arg(long="da", short('d'), default_value = "0xFF",value_parser=maybe_hex::<u8>)]
    /// Adapter Address (used for packets send and transport protocol)
    pub destination_address: u8,

    #[arg(long)]
    /// J1939-21 use application level transport protocol
    pub j1939_tp: bool,

    #[arg(long = "timeout", short('t'), default_value = "2000")]
    /// Timeout in ms
    pub timeout: u64,

    #[arg(long, short('v'), default_value = "false")]
    pub verbose: bool,

    #[clap(subcommand)]
    command: CanCommand,
}
impl CanCan {
    fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout)
    }
}
pub struct CanContext {
    pub can_can: CanCan,
    pub connection: Box<dyn Connection>,
}

#[derive(Subcommand, Debug, Clone)]
enum CanCommand {
    /// Dump Vector ASC compatible log to stdout.
    Log,
    /// Used for testing.  Requires another instance to send or ping this source address.
    Server,
    /// Latency test. Ping [da] with as many requests as it will respond to.
    Ping,
    /// Bandwidth test.  Send as much data to [da] with as many requests as it will respond to.
    Bandwidth,
    /// Send arbitrary CAN message
    Send {
        /// ID 29 bit (dec or 0xhex)
        #[arg( value_parser=maybe_hex::<u32>)]
        id: u32,
        /// Payload (dec or 0xhex u64)
        #[arg( value_parser=maybe_hex::<u64>)]
        payload: u64,
    },
    /// Read the VIN.
    Vin,
    /// Common UDS requests. See "uds --help" for more.
    Uds {
        #[command(subcommand)]
        uds: Uds,
    },
    /// Common J1939 requests. See "j1939 --help" for more.
    J1939 {
        /// Use application J1939-21 tranport protocol. Defaults to using the adapter, but only RP1210 supports adapter J1939-21 TP.
        #[arg(long, short = 't')]
        transport_protocol: bool,

        #[command(subcommand)]
        j1939: J1939,
    },
}

fn hex_array(arg: &str) -> Result<Box<[u8]>, std::num::ParseIntError> {
    todo!()
    //Ok(Box::new([0, 0, 0]))
}

#[derive(Parser, Debug, Clone)]
pub enum ConnectionDescriptor {
    /// List avaliable adapters
    List {},
    /// Simulation - TODO
    Sim {},
    /// SAE J2534 - TODO
    J2534 {},
    /// Linux "socketcan" interface. Modules must already be loaded.
    #[cfg(target_os = "linux")]
    SocketCan {
        /// device: 'can0'
        //#[arg(long, short('d'))]
        dev: String,

        /// speed: '500000', '250000'
        #[arg(long, short('s'), default_value = "500000")]
        speed: u64,
    },
    /// SLCAN interface
    SLCAN {
        /// COM port
        port: String,

        // CAN bus speed expressed in kbaud - 10, 20, 50, 100, 125, 250, 500, 800, 1000
        speed: u32,
    },
    /// TMC RP1210 interface for Windows.
    #[cfg(windows)]
    RP1210 {
        /// RP1210 Adapter Identifier
        id: String,

        /// RP1210 Device ID
        device: i16,

        #[arg(long, short('C'), default_value = "J1939:Baud=Auto")]
        /// RP1210 Connection String
        connection_string: String,

        #[arg(long, default_value = "false")]
        app_packetize: bool,
    },
}

impl ConnectionDescriptor {
    pub fn connect(&self) -> Result<Box<dyn Connection>> {
        let connection = self;
        match &connection {
            ConnectionDescriptor::List {} => list_all(),
            ConnectionDescriptor::Sim {} => todo!(),
            ConnectionDescriptor::J2534 {} => todo!(),
            #[cfg(target_os = "linux")]
            ConnectionDescriptor::SocketCan { dev, speed } => {
                Ok(Box::new(SocketCanConnection::new(dev, *speed)?) as Box<dyn Connection>)
            }
            ConnectionDescriptor::SLCAN { port, speed } => Ok(Box::new(Slcan::new(port, *speed)?)),
            #[cfg(windows)]
            ConnectionDescriptor::RP1210 {
                id,
                device,
                connection_string,
                app_packetize,
            } => {
                {
                    let mut cs = rp1210::CONNECTION_STRING.write().unwrap();
                    *cs = connection_string.to_string();
                    let mut ap = rp1210::APP_PACKETIZATION.write().unwrap();
                    *ap = *app_packetize;
                }
                Ok(Box::new(Rp1210::new(id, *device, self.source_address)?) as Box<dyn Connection>)
            }
        }
    }
}

pub fn list_all() -> ! {
    for pd in connection::enumerate_connections().unwrap() {
        eprintln!("{}", pd.name);
        for dd in pd.devices {
            eprintln!("  {}", dd.name);
            for c in dd.connections {
                eprintln!("    {}: {}", c.name(), c.command_line());
            }
        }
    }
    std::process::exit(0);
}

pub fn main() -> Result<()> {
    let can_can = CanCan::parse();

    let connection =
        ConnectionDescriptor::parse_from(std::iter::once("").chain(can_can.connection.split(" ")))
            .connect()?;

    let cli = &mut CanContext {
        can_can,
        connection,
    };
    match cli.can_can.command.clone() {
        CanCommand::Server => {
            server(cli)?;
        }
        CanCommand::Ping => {
            ping(cli)?;
        }
        CanCommand::Send { id, payload } => {
            send(cli, id, &payload.to_be_bytes())?;
        }
        CanCommand::Bandwidth => {
            bandwidth(cli)?;
        }
        CanCommand::Vin => {
            vin(cli)?;
        }
        CanCommand::Log => {
            log(cli)?;
        }
        CanCommand::Uds { uds } => {
            uds.execute(cli).expect("Unable to send UDS");
        }
        CanCommand::J1939 {
            j1939,
            transport_protocol,
        } => {
            j1939.execute(cli, transport_protocol)?;
        }
    }
    Ok(())
}

fn send(can_can: &mut CanContext, id: u32, payload: &[u8]) -> Result<()> {
    let packet = Packet::new(id, payload);
    can_can.connection.send(&packet)?;
    Ok(())
}

const PING_PGN: u32 = 0xFF00;
const SEND_PGN: u32 = 0xFF01;

fn bandwidth(can_can: &mut CanContext) -> Result<()> {
    let source_address = can_can.can_can.source_address;
    let destination_address = can_can.can_can.destination_address;
    let connection = can_can.connection.as_mut();
    let mut sequence: u64 = 0;
    loop {
        let p = J1939Packet::new_packet(
            None,
            1,
            6,
            SEND_PGN,
            destination_address,
            source_address,
            &sequence.to_be_bytes(),
        );
        connection.send(&p.into())?;
        sequence += 1;
    }
}

fn ping(cli: &mut CanContext) -> Result<()> {
    let source_address = cli.can_can.source_address;
    let destination_address = cli.can_can.destination_address;
    let connection = cli.connection.as_mut();
    let mut sequence: u64 = 0;
    let mut complete: u64 = 0;
    let mut last_report = Instant::now();
    let start = last_report;
    loop {
        let p = J1939Packet::new_packet(
            None,
            1,
            6,
            PING_PGN,
            destination_address,
            source_address,
            &sequence.to_be_bytes(),
        )
        .into();
        let mut iter = connection
            .iter_for(Duration::from_secs(1))
            .map(|p| p.into());
        connection.send(&p)?;
        if iter.any(|p: J1939Packet| p.pgn() == PING_PGN && p.source() == destination_address) {
            complete += 1;
        } else {
            eprintln!("FAIL: {p}");
        }
        sequence += 1;
        let now = Instant::now();
        if now - last_report > Duration::from_secs(1) {
            eprintln!(
                "{} {complete}/{sequence}",
                now.duration_since(start).as_millis()
            );
            last_report = now;
        }
    }
}

fn server(cli: &mut CanContext) -> Result<()> {
    let sa = cli.can_can.source_address;
    let mut count: i64 = 0;
    let mut prev: i64 = 0;
    let mut prev_time: SystemTime = SystemTime::now();
    let connection = cli.connection.as_mut();
    let stream = connection
        .iter()
        .filter_map(|o| o.map(|p| p.into()))
        .filter(|p: &J1939Packet| p.source() != sa);

    for p in stream {
        if p.pgn() == PING_PGN {
            count += 1;
            let pong = &J1939Packet::new_packet(None, 1, 6, PING_PGN, p.source(), sa, {
                let this = &p;
                &this.payload
            });
            if count % 10_000 == 0 {
                eprintln!("pong: {p} -> {pong}");
            }
            connection.send(&pong.into())?;
        } else if p.pgn() == SEND_PGN {
            count += 1;
            let this = {
                let mut arr = [0u8; 8];
                arr.copy_from_slice({
                    let this = &p;
                    &this.payload
                });
                i64::from_be_bytes(arr)
            };
            if prev + 1 != this {
                let diff = this - prev;
                eprintln!("skipped: {diff} prev: {prev:X} this: {this:X} packet: {p}");
            }
            prev = this;
            if count % 1_000 == 0 {
                let now = SystemTime::now();
                let rate = 1000.0 / now.duration_since(prev_time)?.as_secs_f64();
                eprintln!("send count: {count} rate: {rate} packet/s");
                prev_time = now;
                connection.send(
                    &J1939Packet::new_packet(
                        None,
                        1,
                        6,
                        SEND_PGN,
                        p.source(),
                        sa,
                        &count.to_be_bytes(),
                    )
                    .into(),
                )?;
            }
        }
    }
    Ok(())
}

fn vin(can_can: &mut CanContext) -> Result<()> {
    let connection = can_can.connection.as_mut();
    {
        eprintln!("request VIN from ECM");
        // start collecting packets
        let mut iter = connection
            .iter_for(Duration::from_secs(5))
            .map(|p| p.into());
        let packets = J1939::receive_tp(connection, 0xF9, false, &mut iter);
        // send request for VIN
        J1939::request(connection, Duration::from_secs(3), true, 0xF9, 0x00, 0xFEEC)?;

        // filter for ECM result
        if let Some(p) = packets
            .filter(|p| {
                p.pgn() == 0xFEEC || [0xEA00, 0xEB00, 0xEC00, 0xE800].contains(&(p.pgn() & 0xFF00))
            })
            .map(|p| {
                eprintln!("   {p}");
                p
            })
            .find(|p| p.pgn() == 0xFEEC && p.source() == 0)
        {
            println!(
                "ECM {:02X} VIN: {}\n{}",
                p.source(),
                String::from_utf8(p.data().into()).unwrap(),
                p
            );
        };
    }
    {
        eprintln!("\nrequest VIN from Broadcast");

        // start collecting packets
        let mut packets = connection
            .iter_for(Duration::from_secs(5))
            .map(|p| p.into());
        let packets = J1939::receive_tp(connection, 0xF9, false, &mut packets);

        // send request for VIN
        J1939::request(connection, Duration::from_secs(3), true, 0xF9, 0xFF, 0xFEEC)?;

        // filter for all results
        packets.filter(|p| p.pgn() == 0xFEEC).for_each(|p| {
            println!(
                "SA: {:02X} VIN: {}",
                p.source(),
                String::from_utf8(p.data().into()).unwrap()
            )
        });
    }
    Ok(())
}

fn log(can_can: &mut CanContext) -> Result<()> {
    let connection = can_can.connection.as_mut();
    let mut iter = connection.iter().flatten().map(|p| p.into());
    let j1939_tp = can_can.can_can.j1939_tp;
    eprintln!("\n\nlog everything for the next 30 days tp:{j1939_tp}");
    if j1939_tp {
        J1939::receive_tp(connection, can_can.can_can.source_address, false, &mut iter)
            .for_each(|p| println!("{p}"));
    } else {
        iter.for_each(|p| println!("{p}"));
    }
    Ok(())
}
