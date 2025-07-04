use std::{
    collections::HashMap,
    io::{self, Write},
    time::Duration,
};

use anyhow::Result;
use clap_num::maybe_hex;

use crate::{connection::Connection, packet::J1939Packet, CanContext};
use clap::Parser;

#[derive(Parser, Debug, Clone)]
pub enum J1939 {
    Request {
        /// SA in hex
        #[arg(value_parser=maybe_hex::<u8>)]
        sa: u8,
        /// DA in hex
        #[arg(value_parser=maybe_hex::<u8>)]
        da: u8,
        /// 24 bit PGN as hex
        #[arg(value_parser=maybe_hex::<u32>)]
        pgn: u32,
    },
    AddressClaim {
        did: u16,
    },
}
impl J1939 {
    pub fn execute(&self, can_can: &mut CanContext, transport_protocol: bool) -> Result<()> {
        let connection = can_can.connection.as_mut();
        match self {
            J1939::Request { sa, da, pgn } => {
                let packet = request(connection, transport_protocol, *sa, *da, *pgn)?;
                let s = packet.map_or("No Response".to_string(), |p| format!("{p}"));
                println!("{s}");
                Ok(())
            }
            J1939::AddressClaim { did } => todo!(),
        }
    }
}

fn request(
    connection: &mut (dyn Connection),
    transport_protocol: bool,
    sa: u8,
    da: u8,
    pgn: u32,
) -> Result<Option<J1939Packet>, anyhow::Error> {
    let iter = connection.iter_for(Duration::from_secs_f32(2.0));
    let packet = J1939Packet::new(
        None,
        0,
        0x18EA0000 | ((da as u32) << 8) | (sa as u32),
        pgn.to_le_bytes()[0..3].into(),
    );
    connection.send(&packet)?;

    let mut response_id = pgn << 8 | (da as u32);
    if pgn < 0xF000 {
        response_id |= (sa as u32) << 8;
    }
    let predicate = |p:&J1939Packet| p.id() & 0xFFFFFF == response_id;

    let packet = if transport_protocol {
        receive_tp(connection, sa, false, iter).find(predicate)
    } else {
        iter.into_iter().find(predicate)
    };
    Ok(packet)
}

struct TPDescriptor {
    size: u16,
    count: u8,
    pgn: u32,
    data: Vec<u8>,
    timestamp: Option<u32>,
}

pub fn receive_tp(
    connection: &mut dyn Connection,
    addr: u8,
    passive: bool,
    iter: Box<dyn Iterator<Item = J1939Packet>>,
) -> impl Iterator<Item = J1939Packet> + '_ {
    let ds_control_p = 0xEC0000 | (addr as u32) << 8;
    let ds_data_p = 0xEB0000 | (addr as u32) << 8;
    let bam_control_p = 0xECFF00;
    let bam_data_p = 0xEBFF00;

    let mut bam: HashMap<u8, TPDescriptor> = HashMap::new();
    let mut ds: HashMap<u8, TPDescriptor> = HashMap::new();

    let x = iter.flat_map(move |p| {
        let r = if p.id() & 0xFFFF00 == bam_control_p {
            control(connection, &mut bam, true, &p);
            None
        } else if p.id() & 0xFFFF00 == ds_control_p {
            control(connection, &mut ds, passive, &p);
            None
        } else if p.id() & 0xFFFF00 == bam_data_p {
            data(&mut bam, &p)
        } else if p.id() & 0xFFFF00 == ds_data_p {
            data(&mut ds, &p)
        } else {
            None
        };
        if r.is_some() {
            vec![p, r.unwrap()].into_iter()
        } else {
            vec![p].into_iter()
        }
    });
    x
}

fn control(
    connection: &mut dyn Connection,
    ds: &mut HashMap<u8, TPDescriptor>,
    passive: bool,
    p: &J1939Packet,
) -> Result<()> {
    let command = p.data()[0];
    if command == 0x20 || command == 0x10 {
        // RTS/BAM
        let mut pgn = p.data()[5..8].to_vec();
        pgn.push(0);
        let size = u16::from_le_bytes((p.data()[1..3]).try_into().unwrap());
        let count = p.data()[3];
        ds.insert(
            p.source(),
            TPDescriptor {
                size,
                count,
                pgn: u32::from_le_bytes(pgn[0..4].try_into().unwrap()),
                data: Vec::new(),
                timestamp: Some(p.time()),
            },
        );
        if !passive {
            // send CTS
            let data = [
                0x11,
                count,
                1,
                0xFF,
                0xFF,
                p.data()[5],
                p.data()[6],
                p.data()[7],
            ];
            connection.send(&J1939Packet::new_packet(
                None,
                1,
                0x6,
                0xEC00,
                p.source(),
                p.dest(),
                &data,
            ))?;
        }
    } else if command == 0xFF {
        // cancel
        ds.remove(&p.source());
    }
    Ok(())
}

fn data(table: &mut HashMap<u8, TPDescriptor>, p: &J1939Packet) -> Option<J1939Packet> {
    let d = table.get_mut(&p.source());
    let r = match d {
        Some(d) => {
            if p.data()[0] == (1 + d.data.len() / 7) as u8 {
                d.data.extend(p.data()[1..].iter());
            }
            if d.data.len() >= d.size as usize {
                d.data.truncate(d.size as usize);
                Some(J1939Packet::new_packet(
                    d.timestamp,
                    0,
                    p.priority(),
                    d.pgn,
                    p.dest(),
                    p.source(),
                    &d.data,
                ))
            } else {
                None
            }
        }
        None => None,
    };
    if r.is_some() {
        table.remove(&p.source());
    }
    r
}
