use std::{default, fmt::Write, time::Duration};

use clap::{parser, Args, CommandFactory, FromArgMatches, Parser};
use multiqueue::MultiQueue;
use packet::J1939Packet;

pub mod multiqueue;
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
    pub fn connect(
        &self,
        bus: MultiQueue<packet::J1939Packet>,
    ) -> Result<rp1210::Rp1210, anyhow::Error> {
        rp1210::Rp1210::new(
            &self.adapter,
            self.device,
            &self.connection_string,
            self.source_address,
            bus.clone(),
        )
    }
}

fn hex8(str: &str) -> Result<u8, std::num::ParseIntError> {
    u8::from_str_radix(str, 16)
}

fn main() -> Result<(), anyhow::Error> {
    let help = rp1210_parsing::list_all_products()
        .unwrap()
        .iter()
        .flat_map(|p| {
            std::iter::once(format!(
                color_print::cstr!("  <b>{}</> <b>{}</>"),
                p.id, p.description
            ))
            .chain(p.devices.iter().map(|p| {
                format!(
                    color_print::cstr!("    --adapter <bold>{}</> --device <bold>{}</>: {}"),
                    p.name, p.id, p.description
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
    let c = &mut command;
    let parse = {
        let mut matches = c.clone().get_matches();
        let res = Cli::from_arg_matches_mut(&mut matches).map_err(|err| err.format(c));
        match res {
            Ok(s) => s,
            Err(e) => e.exit(),
        }
    };

    let bus: MultiQueue<J1939Packet> = MultiQueue::new();
    let mut rp1210 = parse.connection.connect(bus.clone())?;
    let _thread = rp1210.run(None, parse.connection.app_packetize)?;
    bus.iter_for(Duration::from_secs(60 * 60 * 24 * 7))
        .for_each(|p| println!("{}", p));
    Ok(())
}
