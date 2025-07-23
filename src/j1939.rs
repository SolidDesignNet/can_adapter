use std::{collections::HashMap, time::Duration};

use anyhow::{Context, Result};
use clap_num::maybe_hex;

use crate::{connection::Connection, j1939_packet::J1939Packet, packet::Packet, CanContext};
use clap::Parser;
use zerocopy::*;

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
#[derive(Debug)]
struct TPDescriptor {
    size: u16,
    count: u8,
    pgn: u32,
    data: Vec<u8>,
    timestamp: Option<Duration>,
}

impl J1939 {
    pub const TR: std::time::Duration = Duration::from_millis(200);
    pub const TH: std::time::Duration = Duration::from_millis(500);
    pub const T1: std::time::Duration = Duration::from_millis(750);
    pub const T2: std::time::Duration = Duration::from_millis(1250);
    pub const T3: std::time::Duration = Duration::from_millis(1250);
    pub const T4: std::time::Duration = Duration::from_millis(1050);

    pub fn execute(&self, can_can: &mut CanContext, transport_protocol: bool) -> Result<()> {
        let connection = can_can.connection.as_mut();
        match self {
            J1939::Request { sa, da, pgn } => {
                let packet = J1939::request(
                    connection,
                    can_can.can_can.timeout(),
                    transport_protocol,
                    *sa,
                    *da,
                    *pgn,
                )?;
                let s = packet.map_or("No Response".to_string(), |p| format!("{p}"));
                println!("{s}");
                Ok(())
            }
            J1939::AddressClaim { did } => todo!(),
        }
    }

    pub fn request(
        connection: &(dyn Connection),
        duration: Duration,
        transport_protocol: bool,
        sa: u8,
        da: u8,
        pgn: u32,
    ) -> Result<Option<J1939Packet>, anyhow::Error> {
        let iter = connection.iter_for(duration);
        let packet = J1939Packet::new(
            None,
            0,
            0x18EA0000 | ((da as u32) << 8) | (sa as u32),
            pgn.to_le_bytes()[0..3].into(),
        );
        connection.send(&packet.into())?;

        let mut response_id = pgn << 8 | (da as u32);
        if pgn < 0xF000 {
            response_id |= (sa as u32) << 8;
        }
        let predicate = |p: &J1939Packet| p.id() & 0xFFFFFF == response_id;

        let packet = if transport_protocol {
            J1939::receive_tp(connection, sa, false, &mut iter.map(|p| p.into())).find(predicate)
        } else {
            iter.map(|o| o.into()).find(predicate)
        };
        Ok(packet)
    }
    pub fn send(connection: &mut (dyn Connection), packet: &J1939Packet) -> Result<()> {
        if packet.len() > 8 {
            J1939::send_tp(connection, packet)
        } else {
            connection.send(&packet.into())?;
            Ok(())
        }
    }

    fn send_tp(connection: &mut (dyn Connection), packet: &J1939Packet) -> Result<()> {
        if packet.dest() == 0xFF {
            J1939::send_tp_bam(connection, packet)
        } else {
            J1939::send_tp_ds(connection, packet)
        }
    }
    fn send_tp_bam(connection: &mut (dyn Connection), packet: &J1939Packet) -> Result<()> {
        let pgn = packet.pgn();
        let size = packet.len();
        let count = (1 + size / 7) as u8;
        let payload = [
            0x20u8,
            size as u8,
            (size >> 8) as u8,
            count,
            0xFF,
            pgn as u8,
            (pgn >> 8) as u8,
            (pgn >> 16) as u8,
        ];
        let bam = J1939Packet::new(None, 0, 0x18ECFF00 | (packet.source() as u32), &payload);
        connection.send(&bam.into())?;

        for seq in 1..=count {
            let start = (seq as usize - 1) * 7;
            let end = Ord::min(start + 7, packet.len());
            let payload = &[&[seq][..], &packet.payload[start..end]].concat();
            let dt = J1939Packet::new(None, 0, 0x18EBFF00 | (packet.source() as u32), payload);
            connection.send(&dt.into())?;
        }
        Ok(())
    }
    fn send_tp_ds(connection: &mut (dyn Connection), packet: &J1939Packet) -> Result<()> {
        let rx_id = 0xEC0000 | (packet.source() as u32) << 8 | (packet.dest() as u32);

        let pgn = packet.pgn();
        let size = packet.len();
        let count = (1 + size / 7) as u8;
        fn into_j1939packet(p: Packet) -> J1939Packet {
            p.into()
        }
        let mut cts_iter = connection.iter_for(J1939::T3).map(into_j1939packet);
        let control_id = 0x18EC0000 | ((packet.dest() as u32) << 8) | (packet.source() as u32);
        let data_id = 0x18EB0000 | ((packet.dest() as u32) << 8) | (packet.source() as u32);
        let rts = J1939Packet::new(
            None,
            0,
            control_id,
            J1939_21TpCmRts {
                control: 0x10,
                size: size as u16,
                count,
                allowed: 0xFF,
                pgn: pgn.into(),
            }
            .as_bytes(),
        );
        connection.send(&rts.into())?;
        loop {
            let cts = cts_iter
                .find(|p| p.id() & 0xFFFFFF == rx_id)
                .context("CTS not received.")?;
            if cts.payload[0] == 0x13 {
                // end of message
                break;
            }
            if {
                let this = &cts;
                &this.payload
            }[0] == 0xFF {
                todo!();
                //Err("Aborted")
            }
            let to_send = {
                let this = &cts;
                &this.payload
            }[1];
            let next = {
                let this = &cts;
                &this.payload
            }[2];
            for seq in next..(next + to_send) {
                let start = (seq as usize - 1) * 7;
                let end = Ord::min(start + 7, {
                    let this = &packet;
                    &this.payload
                }.len());
                let dt = J1939Packet::new(
                    None,
                    0,
                    data_id,
                    &[&[seq], &{
                        let this = &packet;
                        &this.payload
                    }[start..end]].concat(),
                );
                connection.send(&dt.into())?;
            }
            cts_iter = connection.iter_for(J1939::T3).map(into_j1939packet);
        }
        Ok(())
    }

    pub fn receive_tp<'a>(
        connection: &'a dyn Connection,
        addr: u8,
        passive: bool,
        iter: &'a mut dyn Iterator<Item = J1939Packet>,
    ) -> impl Iterator<Item = J1939Packet> + 'a {
        let ds_control_p = 0xEC0000 | (addr as u32) << 8;
        let ds_data_p = 0xEB0000 | (addr as u32) << 8;
        let bam_control_p = 0xECFF00;
        let bam_data_p = 0xEBFF00;

        let mut bam: HashMap<u8, TPDescriptor> = HashMap::new();
        let mut ds: HashMap<u8, TPDescriptor> = HashMap::new();

        iter.flat_map(move |p| {
            let mut r = if p.id() & 0xFFFF00 == bam_control_p {
                J1939::control(connection, &mut bam, true, &p)
                    .expect("Unable to handle control message {p}");
                Vec::new()
            } else if p.id() & 0xFFFF00 == ds_control_p {
                J1939::control(connection, &mut ds, passive, &p)
                    .expect("Unable to handle control message {p}");
                Vec::new()
            } else if p.id() & 0xFFFF00 == bam_data_p {
                J1939::data(connection, &mut bam, true, &p)
                    .expect("Unable to handle data message {p}")
            } else if p.id() & 0xFFFF00 == ds_data_p {
                J1939::data(connection, &mut ds, false, &p)
                    .expect("Unable to handle data message {p}")
            } else {
                Vec::new()
            };

            r.insert(0, p);
            r.into_iter()
        })
    }

    fn control(
        connection: &dyn Connection,
        table: &mut HashMap<u8, TPDescriptor>,
        passive: bool,
        p: &J1939Packet,
    ) -> Result<()> {
        let command = {
            let this = &p;
            &this.payload
        }[0];
        if command == 0x20 || command == 0x10 {
            // RTS/BAM
            let mut pgn = {
                let this = &p;
                &this.payload
            }[5..8].to_vec();
            pgn.push(0);
            let size = u16::from_le_bytes(({
                let this = &p;
                &this.payload
            }[1..3]).try_into().unwrap());
            let count = {
                let this = &p;
                &this.payload
            }[3];
            let pgn = u32::from_le_bytes(pgn[0..4].try_into().unwrap());
            table.insert(
                p.source(),
                TPDescriptor {
                    size,
                    count,
                    pgn,
                    data: Vec::new(),
                    timestamp: p.time(),
                },
            );
            if !passive {
                // send CTS
                let cts = J1939Packet::new_packet(
                    None,
                    1,
                    0x6,
                    0xEC00,
                    p.source(),
                    p.dest(),
                    J1939_21TpCmCts {
                        control: 0x11,
                        count,
                        next: 1,
                        reserved: 0xFFFF,
                        pgn: pgn.into(),
                    }
                    .as_bytes(),
                );
                connection.send(&cts.into())?;
            }
        } else if command == 0xFF {
            // cancel
            table.remove(&p.source());
        }
        Ok(())
    }

    fn data(
        connection: &dyn Connection,
        table: &mut HashMap<u8, TPDescriptor>,
        passive: bool,
        p: &J1939Packet,
    ) -> Result<Vec<J1939Packet>> {
        let d = table.get_mut(&p.source());
        let r = match d {
            Some(d) => {
                if {
                    let this = &p;
                    &this.payload
                }[0] == (1 + d.data.len() / 7) as u8 {
                    d.data.extend({
                        let this = &p;
                        &this.payload
                    }[1..].iter());
                }

                if d.data.len() >= d.size as usize {
                    d.data.truncate(d.size as usize);

                    let packet = J1939Packet::new_packet(
                        p.time(),
                        0,
                        p.priority(),
                        d.pgn,
                        p.dest(),
                        p.source(),
                        &d.data,
                    );
                    if !passive {
                        let eom = J1939Packet::new_packet(
                            None,
                            1,
                            0x6,
                            0xEC00,
                            p.source(),
                            p.dest(),
                            J1939_21TpCmEOM {
                                control: 0x13,
                                size: d.size,
                                count: d.count,
                                reserved: 0xFF,
                                pgn: d.pgn.into(),
                            }
                            .as_bytes(),
                        );
                        connection.send(&eom.clone().into())?;

                        vec![eom, packet]
                    } else {
                        vec![packet]
                    }
                } else {
                    Vec::new()
                }
            }
            None => Vec::new(),
        };

        if !r.is_empty() {
            table.remove(&p.source());
        }
        Ok(r)
    }
}

#[repr(C, packed)]
#[derive(Immutable, IntoBytes, TryFromBytes)]
struct u24 {
    value: [u8; 3],
}

impl From<u32> for u24 {
    fn from(v: u32) -> Self {
        let mut value = [0u8; 3];
        value.copy_from_slice(&v.as_bytes()[0..3]);
        u24 { value }
    }
}
impl From<u24> for u32 {
    fn from(v: u24) -> Self {
        u32::from_be_bytes([0, v.value[0], v.value[1], v.value[2]])
    }
}

#[repr(C, packed)]
#[derive(Immutable, IntoBytes, TryFromBytes)]
struct J1939_21TpCmRts {
    control: u8,
    size: u16,
    count: u8,
    allowed: u8,
    pgn: u24,
}
#[repr(C, packed)]
#[derive(Immutable, IntoBytes, TryFromBytes)]
struct J1939_21TpCmCts {
    control: u8,
    count: u8,
    next: u8,
    reserved: u16,
    pgn: u24,
}
#[repr(C, packed)]
#[derive(Immutable, IntoBytes, TryFromBytes)]
struct J1939_21TpCmEOM {
    control: u8,
    size: u16,
    count: u8,
    reserved: u8,
    pgn: u24,
}
#[repr(C, packed)]
#[derive(Immutable, IntoBytes, TryFromBytes)]
struct J1939_21TpConnAbort {
    control: u8,
    reason: u8,
    reserved: u24,
    pgn: u24,
}
#[repr(C, packed)]
#[derive(Immutable, IntoBytes, TryFromBytes)]
struct J1939_21TpBAM {
    control: u8,
    size: u16,
    count: u8,
    reserved: u8,
    pgn: u24,
}

#[cfg(test)]
mod tests {
    use std::thread;

    use anyhow::Ok;

    use crate::sim::SimulatedConnection;

    use super::*;
    #[test]
    pub fn send14_bam() -> Result<()> {
        let mut rx_connection = Box::new(SimulatedConnection::new()?);
        let mut tx_connection = rx_connection.clone();

        let mut iter = rx_connection
            .iter_for(Duration::from_secs(2))
            .map(|p| p.into());

        let payload: &[u8] = &[&[0, 0, 0, 1], "Something".as_bytes()].concat()[..];
        let tx = J1939Packet::new(None, 0, 0x18D3FF00, payload);
        thread::spawn(move || {
            let _ = J1939::send(tx_connection.as_mut(), &tx);
        });
        let mut rx_tp = J1939::receive_tp(rx_connection.as_mut(), 0xF9, false, &mut iter);
        let rx = rx_tp.find(|p| p.id() & 0xFFFFFF == 0xD3FF00);
        assert_eq!(payload.to_vec(), rx.unwrap().data());
        Ok(())
    }
    #[test]
    pub fn send14_ds() -> Result<()> {
        let mut rx_connection = Box::new(SimulatedConnection::new()?);
        let mut tx_connection = rx_connection.clone();

        // log everything
        let log = rx_connection.iter_for(Duration::from_secs(3));
        thread::spawn(move || log.for_each(|p| eprintln!("p: {p:?}")));

        let mut iter = rx_connection
            .iter_for(Duration::from_secs(2))
            .map(|p| p.into());

        let payload: &[u8] = &[&[0, 0, 0, 1], "Something".as_bytes()].concat()[..];
        let tx = J1939Packet::new(None, 0, 0x18D3F903, payload);
        let tx2 = tx.clone();
        thread::spawn(move || {
            let _ = J1939::send(tx_connection.as_mut(), &tx);
        });
        let mut rx_tp = J1939::receive_tp(rx_connection.as_mut(), 0xF9, false, &mut iter);
        let rx = rx_tp.find(|p| p.id() & 0xFFFFFF == 0xD3F903);
        eprintln!(" results {tx2:?} -> {rx:?}");
        assert_eq!(payload.to_vec(), rx.unwrap().data());
        Ok(())
    }
}
