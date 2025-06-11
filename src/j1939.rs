use std::{
    io::{self, Write},
    time::Duration,
};

use anyhow::Result;

use crate::{connection::Connection, packet::J1939Packet};
use clap::Parser;

#[derive(Parser, Debug, Clone)]
pub enum J1939 {
    Request {
        /// SA in hex
        #[arg(value_parser=crate::hex8)]
        sa: u8,
        /// DA in hex
        #[arg(value_parser=crate::hex8)]
        da: u8,
        /// 24 bit PGN as hex
        #[arg(value_parser=crate::hex32)]
        pgn: u32,
    },
    AddressClaim { did: u16 },
}
impl J1939 {
    pub fn execute(&self, connection: &mut dyn Connection) -> Result<()> {
        match self {
            J1939::Request { sa, da, pgn } => {
                let mut stream = connection.iter_for(Duration::from_secs_f32(2.0));
                let packet = J1939Packet::new(
                    None,
                    0,
                    0x18EA0000 | ((*da as u32) << 8) | (*sa as u32),
                    pgn.to_be_bytes()[1..].into(),
                );
                eprintln!("{}", packet);
                connection.send(&packet)?;
                let mut response_id = *pgn << 8 | (*sa as u32);
                if *pgn < 0xF000 {
                    response_id |= (*da as u32) << 8;
                }
                let s = stream
                    .find(|p| p.id() == response_id)
                    .map_or("No Response".to_string(), |p| format!("{}", p));
                io::stdout().write_all(s.as_bytes())?;
                Ok(())
            }
            J1939::AddressClaim { did } => todo!(),
        }
    }
}
