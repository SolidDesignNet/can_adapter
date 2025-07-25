use std::{thread, time::Duration};

use anyhow::{anyhow, Result};

use crate::{connection::Connection, packet::Packet, uds::UdsBuffer};

pub struct Iso15765<'a> {
    connection: &'a mut dyn Connection,
    sa: u8,
    da: u8,
    pgn: u32,
    send_header: u32,
    receive_header: u32,
    duration: Duration,
}

impl<'a> Iso15765<'a> {
    // connection should be a Box
    pub fn new(
        connection: &'a mut dyn Connection,
        pgn: u32,
        duration: Duration,
        sa: u8,
        da: u8,
    ) -> Self {
        let sa32 = sa as u32;
        let da32 = da as u32;
        Iso15765 {
            connection,
            sa,
            da,
            pgn,
            duration,
            send_header: pgn << 8 | da32 << 8 | sa32,
            receive_header: pgn << 8 | sa32 << 8 | da32,
        }
    }
    pub fn send(&mut self, req: &UdsBuffer) -> Result<()> {
        if req.len() > 8 {
            self.transport_send(req)?;
        } else {
            let mut payload = [&[req.len() as u8][..], &req[..]].concat();
            // pad out to 8 bytes
            while payload.len() < 8 {
                payload.push(0xFF);
            }
            let p = Packet::new(self.send_header | 0x18000000, &payload);
            self.connection.send(&p)?;
        }
        Ok(())
    }

    /// This assumes that all ISO15765 is synchronous.
    pub fn receive(&mut self, iter: &mut dyn Iterator<Item = Packet>) -> Result<Option<UdsBuffer>> {
        let p = iter
            .filter(|p| p.id & 0xFFFFFF == self.receive_header)
            .next();
        if let Some(p) = p {
            if p.payload[0] & 0xF0 == 0x00 {
                Ok(Some(p.payload[1..(1 + p.payload[0] as usize)].to_vec()))
            } else {
                self.transport_receive(&p)
            }
        } else {
            Err(anyhow!("No response"))
        }
    }

    pub fn send_receive(&mut self, req: &UdsBuffer) -> Result<Option<UdsBuffer>> {
        let mut iter = self.connection.iter_for(self.duration);
        self.send(req)?;
        self.receive(&mut iter)
    }

    fn transport_send(&mut self, req: &UdsBuffer) -> Result<()> {
        // send first frame
        let size = req.len();
        let payload = [&[0x10 | (0xF & (size >> 8) as u8), size as u8], &req[0..6]].concat();
        let first_frame = Packet::new(self.send_header, &payload);
        let mut flow_control_stream = self.connection.iter_for(Duration::from_secs(2));
        self.connection.send(&first_frame)?;

        // response to flow control
        let flow_control = flow_control_stream.find(|p| p.id & 0xFFFFFF == self.receive_header);
        match flow_control {
            Some(p) => {
                if p.payload[0] == 0x7F {
                    Err(anyhow!("NACK: {req:?} -> {p}"))
                } else if p.payload[0] != 0x30 {
                    Err(anyhow!(
                        "Unexpected: {req:?} -> {p} should this be ignored?"
                    ))
                } else {
                    // validate response?

                    // FIXME use block size and flow control!
                    let block_size = p.payload[1];

                    let interpacket_delay = p.payload[2] as u64;
                    let interpacket_delay = if interpacket_delay > 0xF0 && interpacket_delay < 0xFA
                    {
                        Duration::from_micros(100 * (0xF & interpacket_delay))
                    } else {
                        Duration::from_millis(interpacket_delay)
                    };

                    // packet size of 9 means 1 consecutive packet
                    // packet size of 13 means 1 consecutive packet
                    // packet size of 14 means 2 consecutive packets
                    // packet size of 20 means 2 consecutive packets
                    // packet size of 21 means 3 consecutive packets

                    // First consecutive packet sequence is 1 and max is 0 (really. Look it up.)
                    let frames = 1 + size / 7;
                    for sequence in 1..frames {
                        thread::sleep(interpacket_delay);
                        let offset = 6 + (sequence - 1) * 7;
                        let end = Ord::min(7 + offset, req.len());
                        let mut payload =
                            [&[0x20 | (sequence as u8 & 0xF)], &req[offset..end]].concat();
                        while payload.len() < 8 {
                            payload.push(0xFF);
                        }
                        let consecutive = Packet::new(self.send_header, &payload);
                        self.connection.send(&consecutive)?;
                    }

                    Ok(())
                }
            }
            None => Err(anyhow!("No response to: {req:X?}",)),
        }
    }

    fn transport_receive(&mut self, packet: &Packet) -> Result<Option<Vec<u8>>> {
        let stream = self.connection.iter_for(self.duration);

        // send flow control
        let payload = [0x30, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let flow_control = Packet::new(self.send_header, &payload);
        self.connection.send(&flow_control)?;

        // receive all payload
        let mut result = Vec::new();
        result.extend(packet.payload[2..].iter());

        let len = (0xf & packet.payload[0] as u32) << 8 | (packet.payload[1] as u32);
        let frames = 1 + (len - 6) / 7;
        stream
            .filter(|p| p.id & 0xFFFFFF == self.receive_header)
            // exit as soon as we have all the frames
            .take(frames as usize)
            .for_each(|p| result.extend(p.payload[1..].iter()));
        // trim padding
        result.truncate(len as usize);
        Ok(Some(result))
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Ok;

    use crate::sim::SimulatedConnection;

    use super::*;
    #[test]
    fn send8() -> Result<()> {
        let mut connection = SimulatedConnection::new()?;
        let mut stream = connection.iter_for(Duration::from_secs(2));
        let mut tp = Iso15765::new(&mut connection, 0xDA00, Duration::from_secs(2), 0xF9, 0);
        tp.send(&[1, 2, 3].to_vec())?;
        let packet = stream.find(|p| p.id == 0x18DA00F9).unwrap();
        eprintln!("packet {packet:X?}",);
        assert_eq!(0x18DA00F9, packet.id);
        assert_eq!(
            [0x03, 0x01, 0x02, 0x03, 0xFF, 0xFF, 0xFF, 0xFF],
            packet.payload[0..8]
        );
        Ok(())
    }
    #[test]
    fn send_receive() -> Result<()> {
        let mut connection = SimulatedConnection::new()?;

        let log = connection.iter();
        thread::spawn(move || log.filter(|p| p.is_some()).for_each(|p| eprintln!("{p:?}")));

        let mut tx_connection = connection.clone();
        let mut stream = tx_connection.iter_for(Duration::from_secs(2));
        thread::spawn(move || {
            let mut tp = Iso15765::new(&mut tx_connection, 0xDA00, Duration::from_secs(2), 0, 0xF9);
            let rx = tp.receive(&mut stream).unwrap().unwrap();
            eprintln!(" rx: {rx:?}");
            let tx = rx.iter().map(|u| u + 3).collect();
            eprintln!(" tx: {tx:?}");

            tp.send(&tx).expect("Failed to send");
        });

        let mut tp = Iso15765::new(&mut connection, 0xDA00, Duration::from_secs(2), 0xF9, 0);
        let buf = tp.send_receive(&[1, 2, 3].to_vec())?;
        assert_eq!(
            vec![0x04u8, 0x05, 0x06],
            buf.unwrap()
        );
        Ok(())
    }
    #[test]
    fn send14() -> Result<()> {
        const DURATION: Duration = Duration::from_secs(2_000);
        let mut rx_connection = Box::new(SimulatedConnection::new()?);
        let mut tx_connection = rx_connection.clone();

        let mut stream = rx_connection.iter_for(DURATION);

        thread::spawn(move || {
            let mut tx_tp = Iso15765::new(tx_connection.as_mut(), 0xDA00, DURATION, 0xF9, 0);
            tx_tp.send(&[0x55; 14].to_vec()).expect("Failed to send");
        });

        let mut rx_tp = Iso15765::new(rx_connection.as_mut(), 0xDA00, DURATION, 0, 0xF9);
        let packet = rx_tp.receive(stream.by_ref())?;

        assert_eq!([0x55; 14].to_vec(), packet.unwrap());
        Ok(())
    }
}
