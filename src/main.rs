use std::time::Duration;

use clap::{Parser, Subcommand};
use connection::Connection;
use packet::J1939Packet;
use rp1210::Rp1210;
use socketcanconnection::SocketCanConnection;

pub mod bus;
pub mod connection;
pub mod packet;
pub mod rp1210;
pub mod rp1210_parsing;
pub mod sim;
pub mod socketcanconnection;

#[derive(Parser, Debug, Default, Clone)]
pub struct Cli {
    #[command(subcommand)]
    pub connection: Descriptors,

    #[arg(long="sa", short('a'), default_value = "F9",value_parser=hex8)]
    /// Adapter Address (used for packets send and transport protocol)
    pub source_address: u8,

    #[arg(long, short('v'), default_value = "false")]
    pub verbose: bool,
}
#[derive(Subcommand, Debug, Default, Clone)]
pub enum Descriptors {
    #[default]
    /// List avaliable adapters
    List,
    /// Simulation - TODO
    Sim {},
    /// SAE J2534 - TODO
    J2534 {},
    /// Linux "socketcan" interface. Modules must already be loaded.
    #[command(name="socketcan")]
    SocketCan {
        /// device: 'can0'
        //#[arg(long, short('d'))]
        dev: String,

        /// speed: '500000', '250000'
        #[arg(long, short('s'), default_value = "500000")]
        speed: u64,
    },
    /// TMC RP1210 interface for Windows.
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

impl Cli {
    fn connect(&self) -> Result<Box<dyn Connection>, anyhow::Error> {
        match &self.connection {
            Descriptors::List => list_all(),
            Descriptors::Sim {} => todo!(),
            Descriptors::J2534 {} => todo!(),
            Descriptors::SocketCan { dev, speed } => {
                Ok(Box::new(SocketCanConnection::new(&dev, *speed)?) as Box<dyn Connection>)
            }
            Descriptors::RP1210 {
                id,
                device,
                connection_string,
                app_packetize,
            } => Ok(Box::new(Rp1210::new(
                id,
                *device,
                connection_string,
                self.source_address,
                *app_packetize,
            )?) as Box<dyn Connection>),
        }
    }
}

fn list_all() -> ! {
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

pub fn main() -> Result<(), anyhow::Error> {
    // open the adapter
    let mut connection = Cli::parse().connect()?;
    {
        // request VIN from ECM
        // start collecting packets
        let mut packets = connection.iter_for(Duration::from_secs(2));
        // send request for VIN
        connection.send(&J1939Packet::new(None, 1, 0x18EA00F9, &[0xEC, 0xFE, 0x00]))?;

        // filter for ECM result
        packets
            .find(|p| p.pgn() == 0xFEEC && p.source() == 0)
            // log the VIN
            .map(|p| {
                print!(
                    "ECM {:02X} VIN: {}\n{}",
                    p.source(),
                    String::from_utf8(p.data().into()).unwrap(),
                    p
                )
            });
    }
    {
        // request VIN from Broadcast
        // start collecting packets
        let packets = connection.iter_for(Duration::from_secs(5));

        // send request for VIN
        connection.send(&J1939Packet::new(None, 1, 0x18EAFFF9, &[0xEC, 0xFE, 0x00]))?;
        // filter for all results
        packets
            .filter(|p| p.pgn() == 0xFEEC)
            // log the VINs
            .for_each(|p| {
                println!(
                    "SA: {:02X} VIN: {}",
                    p.source(),
                    String::from_utf8(p.data().into()).unwrap()
                )
            });
    }
    // log everything for the next 30 days
    connection
        .iter_for(Duration::from_secs(60 * 60 * 24 * 30))
        .for_each(|p| println!("{}", p));
    Ok(())
}
