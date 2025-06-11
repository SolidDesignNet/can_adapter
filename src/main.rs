use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;
use clap::{Parser, Subcommand};
use clap_num::maybe_hex;
use connection::Connection;
use packet::J1939Packet;
use slcan::Slcan;

pub mod connection;
pub mod j1939;
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

#[derive(Debug, Parser)] // requires `derive` feature
#[command(name = "cancan")]
#[command(about = "CAN tool", long_about = None)]
struct Cli {
    #[arg(long, short('c'), default_value = "false")]
    connction_help: bool,
    pub connection: String,

    #[arg(long="sa", short('a'), default_value = "F9",value_parser=hex8)]
    /// Adapter Address (used for packets send and transport protocol)
    pub source_address: u8,

    #[arg(long, short('v'), default_value = "false")]
    pub verbose: bool,

    #[clap(subcommand)]
    command: CanCommand,
}

#[derive(Subcommand, Debug, Clone)]
enum CanCommand {
    /// Dump Vector ASC compatible log to stdout.
    Log,
    /// Used for testing.  Requires another instance to send or ping this source address.
    Server,
    /// Latency test. Ping [da] with as many requests as it will respond to.
    Ping {
        #[arg(long="da", short('d'), default_value = "00",value_parser=hex8)]
        /// Adapter Address (used for packets send and transport protocol)
        destination_address: u8,
    },
    /// Bandwidth test.  Send as much data to [da] with as many requests as it will respond to.
    Bandwidth {
        // Destination Address
        #[arg(long="da", short('d'), default_value = "00",value_parser=hex8)]
        destination_address: u8,
    },
    /// Send arbitrary CAN message
    Send {
        /// ID 29 bit hex
        #[arg( value_parser=maybe_hex::<u32>)]
        id: u32,
        /// Payload as hex u64
        #[arg( value_parser=maybe_hex::<u64>)]
        payload: u64,
    },
    /// Read the VIN.
    Vin,
    /// Common UDS requests
    Uds {
        #[command(subcommand)]
        uds: Uds,
    },
    /// Common J1939 requests
    J1939 {
        #[command(subcommand)]
        j1939: J1939,
    },
}

fn hex_array(arg: &str) -> Result<Box<[u8]>, std::num::ParseIntError> {
    Ok(Box::new([0, 0, 0]))
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
                eprintln!("    {}", c.name());
                eprintln!("      can_adapter {}", c.command_line());
            }
        }
    }
    std::process::exit(0);
}

fn hex8(str: &str) -> Result<u8, std::num::ParseIntError> {
    u8::from_str_radix(str, 16)
}
fn hex32(str: &str) -> Result<u32, std::num::ParseIntError> {
    u32::from_str_radix(str, 16)
}

pub fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut connection =
        ConnectionDescriptor::parse_from(std::iter::once("").chain(cli.connection.split(" ")))
            .connect()?;
    let connection = connection.as_mut();
    // open the adapter
    match cli.command {
        CanCommand::Server => {
            server(connection, cli.source_address)?;
        }
        CanCommand::Ping {
            destination_address,
        } => {
            ping(connection, cli.source_address, destination_address)?;
        }
        CanCommand::Send { id, payload } => {
            send(connection, id, &payload.to_be_bytes())?;
        }
        CanCommand::Bandwidth {
            destination_address,
        } => {
            bandwidth(connection, cli.source_address, destination_address)?;
        }
        CanCommand::Vin => {
            vin(connection, cli.source_address)?;
        }
        CanCommand::Log => {
            log(connection)?;
        }
        CanCommand::Uds { uds } => uds.execute(connection)?,
        CanCommand::J1939 { j1939 } => j1939.execute(connection)?,
    }
    Ok(())
}

fn send(connection: &mut (dyn Connection + 'static), id: u32, payload: &[u8]) -> Result<()> {
    let packet = J1939Packet::new(None, 1, id, payload);
    connection.send(&packet)?;
    Ok(())
}

const PING_PGN: u32 = 0xFF00;
const SEND_PGN: u32 = 0xFF01;

fn bandwidth(
    connection: &mut dyn Connection,
    source_address: u8,
    destination_address: u8,
) -> Result<()> {
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
        connection.send(&p)?;
        sequence += 1;
    }
}

fn ping(
    connection: &mut dyn Connection,
    source_address: u8,
    destination_address: u8,
) -> Result<()> {
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
        );
        let mut i = connection.iter_for(Duration::from_secs(1));
        connection.send(&p)?;
        if i.any(|p| p.pgn() == PING_PGN && p.source() == destination_address) {
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

fn server(connection: &mut dyn Connection, sa: u8) -> Result<()> {
    let mut count: i64 = 0;
    let mut prev: i64 = 0;
    let mut prev_time: SystemTime = SystemTime::now();
    let stream = connection
        .iter()
        .filter_map(|o| o.filter(|p| p.source() != sa));

    for p in stream {
        if p.pgn() == PING_PGN {
            count += 1;
            let pong = &J1939Packet::new_packet(None, 1, 6, PING_PGN, p.source(), sa, p.data());
            if count % 10_000 == 0 {
                eprintln!("pong: {p} -> {pong}");
            }
            connection.send(pong)?;
        } else if p.pgn() == SEND_PGN {
            count += 1;
            let this = {
                let mut arr = [0u8; 8];
                arr.copy_from_slice(p.data());
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
                connection.send(&J1939Packet::new_packet(
                    None,
                    1,
                    6,
                    SEND_PGN,
                    p.source(),
                    sa,
                    &count.to_be_bytes(),
                ))?;
            }
        }
    }
    Ok(())
}

fn vin(connection: &mut dyn Connection, source_address: u8) -> Result<()> {
    {
        eprintln!("request VIN from ECM");
        // start collecting packets
        let packets = connection.iter_for(Duration::from_secs(5));
        // send request for VIN
        connection.send(&J1939Packet::new_packet(
            None,
            1,
            6,
            0xEA00,
            0,
            source_address,
            &[0xEC, 0xFE, 0x00],
        ))?;

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
            )
        }
    }
    {
        eprintln!("\nrequest VIN from Broadcast");
        // start collecting packets
        let packets = connection.iter_for(Duration::from_secs(5));

        // send request for VIN
        connection.send(&J1939Packet::new_packet(
            None,
            1,
            6,
            0xEAFF,
            0xFF,
            source_address,
            &[0xEC, 0xFE, 0x00],
        ))?;
        // filter for all results
        packets
            .filter(|p| p.pgn() == 0xFEEC || p.pgn() == 0xEAFF || p.pgn() & 0xFF00 == 0xE800)
            .map(|p| {
                eprintln!("   {p}");
                p
            })
            // log the VINs
            .filter(|p| p.pgn() == 0xFEEC)
            .for_each(|p| {
                println!(
                    "SA: {:02X} VIN: {}",
                    p.source(),
                    String::from_utf8(p.data().into()).unwrap()
                )
            });
    }
    Ok(())
}

fn log(connection: &dyn Connection) -> Result<()> {
    eprintln!("\n\nlog everything for the next 30 days");
    connection
        .iter()
        .flatten() //_for(Duration::from_secs(60 * 60 * 24 * 30))
        .for_each(|p| println!("{p}"));
    Ok(())
}
