use std::{fmt::Write, time::Duration};

use clap::{Args, CommandFactory, FromArgMatches, Parser};
use connection::Connection;
use packet::J1939Packet;

pub mod bus;
pub mod connection;
pub mod packet;

#[cfg_attr(
    not(all(target_pointer_width = "32", target_os = "windows")),
    path = "sim.rs"
)]
#[cfg_attr(
    all(target_pointer_width = "32", target_os = "windows"),
    path = "rp1210.rs"
)]
pub mod rp1210;
pub mod rp1210_parsing;

#[derive(Parser, Debug, Default, Clone)]
pub struct Cli {
    #[command(flatten)]
    pub connection: ConnectionDescriptor,
}
#[derive(Args, Debug, Default, Clone)]
pub struct ConnectionDescriptor {
    /// RP1210 Adapter Identifier
    #[arg(long, short('D'))]
    pub adapter: String,

    /// RP1210 Device ID
    #[arg(long, short('d'))]
    pub device: i16,

    #[arg(long, short('C'), default_value = "J1939:Baud=Auto")]
    /// RP1210 Connection String
    pub connection_string: String,

    #[arg(long="sa", short('a'), default_value = "F9",value_parser=hex8)]
    /// RP1210 Adapter Address (used for packets send and transport protocol)
    pub source_address: u8,

    #[arg(long, short('v'), default_value = "false")]
    pub verbose: bool,

    #[arg(long, default_value = "false")]
    pub app_packetize: bool,
}

impl ConnectionDescriptor {
    pub fn connect(&self) -> Result<impl Connection, anyhow::Error> {
        // FIXME don't assume RP1210.  Also support J2534
        rp1210::Rp1210::new(
            &self.adapter,
            self.device,
            None,
            &self.connection_string,
            self.source_address,
            false,
        )
    }
}

fn hex8(str: &str) -> Result<u8, std::num::ParseIntError> {
    u8::from_str_radix(str, 16)
}

pub fn main() -> Result<(), anyhow::Error> {
    // parse command
    let help = rp1210_parsing::list_all_products()
        .unwrap()
        .iter()
        .flat_map(|p| {
            std::iter::once(format!(
                color_print::cstr!("  <b>{}</> <b>{}</>"),
                p.id, p.description
            ))
            .chain(p.devices.iter().map(|dev| {
                format!(
                    color_print::cstr!("    --adapter <bold>{}</> --device <bold>{}</>: {}"),
                    p.id, dev.id, dev.description
                )
            }))
        })
        .collect::<Vec<String>>()
        .join("\n");

    // inline Command::parse() to override the usage with dynamic content
    let mut command = Cli::command();
    let mut usage = command.render_usage();
    usage.write_str(color_print::cstr!("\n\n<bold>RP1210 Devices:<bold>\n"))?;
    usage.write_str(help.as_str())?;
    command = command.override_usage(usage);
    let parse = {
        let mut matches = command.clone().get_matches();
        let res = Cli::from_arg_matches_mut(&mut matches).map_err(|err| err.format(&mut command));
        match res {
            Ok(s) => s,
            Err(e) => e.exit(),
        }
    };

    // open the adapter
    let mut rp1210 = parse.connection.connect()?;

    {// request VIN from ECM
    // start collecting packets
    let mut packets = rp1210.iter_for(Duration::from_secs(2));
    // send request for VIN
    rp1210.send(&J1939Packet::new(1, 0x18EA00F9, &[0xEC, 0xFE, 0x00]))?;

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
            },
        );
    }
{    // request VIN from Broadcast
    // start collecting packets
    let packets = rp1210.iter_for(Duration::from_secs(5));

    // send request for VIN
    rp1210.send(&J1939Packet::new(1, 0x18EAFFF9, &[0xEC, 0xFE, 0x00]))?;
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
    rp1210
        .iter_for(Duration::from_secs(60 * 60 * 24 * 30))
        .for_each(|p| println!("{}", p));
    Ok(())
}
