use std::time::Duration;

use clap::Parser;
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
pub struct ConnectionDescriptor {
    /// RP1210 Adapter Identifier
    adapter: String,

    /// RP1210 Device ID
    device: i16,

    #[arg(long, default_value = "J1939:Baud=Auto")]
    /// RP1210 Connection String
    connection_string: String,

    #[arg(long, default_value = "F9",value_parser=hex8)]
    /// RP1210 Adapter Address (used for packets send and transport protocol)
    address: u8,

    #[arg(long, short, default_value = "false")]
    verbose: bool,
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
            self.address,
            bus.clone(),
        )
    }
}

fn hex8(str: &str) -> Result<u8, std::num::ParseIntError> {
    u8::from_str_radix(str, 16)
}

fn main() -> Result<(), anyhow::Error> {
    let bus: MultiQueue<J1939Packet> = MultiQueue::new();
    let mut rp1210 = ConnectionDescriptor::parse().connect(bus.clone())?;
    rp1210.run();
    bus.iter_for(Duration::from_secs(60 * 60 * 24 * 7))
        .for_each(|p| println!("{}", p));
    Ok(())
}
