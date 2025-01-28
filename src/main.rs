use std::{ops::Deref, time::Duration};

use clap::{Parser, Subcommand};
use connection::Connection;
use packet::J1939Packet;
use slcan::Slcan;

pub mod bus;
pub mod connection;
pub mod packet;
pub mod sim;
pub mod slcan;

#[cfg(windows)]
pub mod rp1210;
#[cfg(windows)]
use rp1210::Rp1210;

#[cfg(target_os = "linux")]
pub mod socketcanconnection;
#[cfg(target_os = "linux")]
use socketcanconnection::SocketCanConnection;

#[derive(Parser, Debug, Default, Clone)]
pub struct ThisCli {
    #[command(flatten)]
    args: Cli,

    /// before logging demonstrate reading VIN
    #[arg(long)]
    vin: bool,
}

impl Deref for ThisCli {
    type Target = Cli;

    fn deref(&self) -> &Self::Target {
        &self.args
    }
}

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
    #[command(name = "socketcan")]
    #[cfg(target_os = "linux")]
    SocketCan {
        /// device: 'can0'
        //#[arg(long, short('d'))]
        dev: String,

        /// speed: '500000', '250000'
        #[arg(long, short('s'), default_value = "500000")]
        speed: u64,
    },
    /// SLCAN interface for Windows.
    #[cfg(windows)]
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

impl Cli {
    fn connect(&self) -> Result<Box<dyn Connection>, anyhow::Error> {
        eprintln!("connecting {:?}", self.connection);
        match &self.connection {
            Descriptors::List => list_all(),
            Descriptors::Sim {} => todo!(),
            Descriptors::J2534 {} => todo!(),
            #[cfg(target_os = "linux")]
            Descriptors::SocketCan { dev, speed } => {
                Ok(Box::new(SocketCanConnection::new(&dev, *speed)?) as Box<dyn Connection>)
            }
            #[cfg(windows)]
            Descriptors::SLCAN { port, speed } => Ok(Box::new(Slcan::new(port, *speed)?)),
            #[cfg(windows)]
            Descriptors::RP1210 {
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
    let args = ThisCli::parse();
    let mut connection = args.connect()?;
    if args.vin {
        {
            eprintln!("request VIN from ECM");
            // start collecting packets
            let packets = connection.iter_for(Duration::from_secs(2));
            // send request for VIN
            connection.send(&J1939Packet::new(None, 1, 0x18EA00F9, &[0xEC, 0xFE, 0x00]))?;

            // filter for ECM result
            packets
                .filter(|p| p.pgn() == 0xFEEC  || [0xEA00,0xEB00,0xEC00,0xE800].contains(&(p.pgn() & 0xFF00)))
                .map(|p| {
                    eprintln!("   {p}");
                    p
                })
                .find(|p| p.pgn() == 0xFEEC && p.source() == 0)
                // log the VIN
                .map(|p| {
                    println!(
                        "ECM {:02X} VIN: {}\n{}",
                        p.source(),
                        String::from_utf8(p.data().into()).unwrap(),
                        p
                    )
                });
        }
        {
            eprintln!("\nrequest VIN from Broadcast");
            // start collecting packets
            let packets = connection.iter_for(Duration::from_secs(2));

            // send request for VIN
            connection.send(&J1939Packet::new(None, 1, 0x18EAFFF9, &[0xEC, 0xFE, 0x00]))?;
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
    }

    eprintln!("\n\nlog everything for the next 30 days");
    connection
        .iter()
        .filter_map(|p|p) //_for(Duration::from_secs(60 * 60 * 24 * 30))
        .for_each(|p| println!("{p}"));
    Ok(())
}
