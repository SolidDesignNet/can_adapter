use std::{
    io::{self, Write},
    time::Duration,
};

use anyhow::Result;

use crate::{connection::Connection, packet::J1939Packet,  CanContext};
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
    AddressClaim {
        did: u16,
    },
}
impl J1939 {
    pub fn execute(&self, can_can: &mut CanContext) -> Result<()> {
        let connection = can_can.connection.as_mut();
        match self {
            J1939::Request { sa, da, pgn } => {
                let packet = request(connection, *sa, *da, *pgn)?;
                let s = packet.map_or("No Response".to_string(), |p| format!("{p}"));
                io::stdout().write_all(s.as_bytes())?;
                Ok(())
            }
            J1939::AddressClaim { did } => todo!(),
        }
    }
}

fn request(
    connection: &mut (dyn Connection),
    sa: u8,
    da: u8,
    pgn: u32,
) -> Result<Option<J1939Packet>, anyhow::Error> {
    let mut stream = connection.iter_for(Duration::from_secs_f32(2.0));
    let packet = J1939Packet::new(
        None,
        0,
        0x18EA0000 | ((da as u32) << 8) | (sa as u32),
        pgn.to_be_bytes()[1..].into(),
    );
    eprintln!("{packet}");
    connection.send(&packet)?;
    let mut response_id = pgn << 8 | (sa as u32);
    if pgn < 0xF000 {
        response_id |= (da as u32) << 8;
    }
    let packet = stream.find(|p| p.id() == response_id);
    Ok(packet)
}
