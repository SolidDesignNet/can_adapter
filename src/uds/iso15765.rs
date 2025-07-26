use std::{thread, time::Duration};

use anyhow::{anyhow, Result};

use crate::{connection::Connection, packet::Packet};

pub struct Iso15765<'a> {
    connection: &'a dyn Connection,
    send_header: u32,
    receive_header: u32,
    duration: Duration,
}

impl<'a> Iso15765<'a> {
    pub fn new(
        connection: &'a dyn Connection,
        pgn: u32,
        duration: Duration,
        sa: u8,
        da: u8,
    ) -> Self {
        let sa32 = sa as u32;
        let da32 = da as u32;
        Iso15765 {
            connection,
            duration,
            send_header: 0x18000000 | pgn << 8 | da32 << 8 | sa32,
            receive_header: pgn << 8 | sa32 << 8 | da32,
        }
    }
    pub fn send(&self, request: &[u8]) -> Result<()> {
        if request.len() > 8 {
            self.transport_send(request)?;
        } else {
            let mut payload = [&[request.len() as u8], request].concat();
            // pad out to 8 bytes
            while payload.len() < 8 {
                payload.push(0xFF);
            }
            let p = Packet::new(self.send_header, &payload);
            self.connection.send(&p)?;
        }
        Ok(())
    }

    /// This assumes that all ISO15765 is synchronous.
    pub fn receive(&self, iter: &mut impl Iterator<Item = Packet>) -> Result<Option<Vec<u8>>> {
        let packet = iter.find(|p| {
            p.id & 0xFFFFFF == self.receive_header
                && (p.payload[0] & 0xF0 == 0 || p.payload[0] & 0xF0 == 0x10)
        });
        if let Some(p) = packet {
            if p.payload[0] & 0xF0 == 0x00 {
                Ok(Some(p.payload[1..(1 + p.payload[0] as usize)].to_vec()))
            } else {
                self.transport_receive(&p)
            }
        } else {
            Err(anyhow!("No response"))
        }
    }

    pub fn send_receive(&self, request: &[u8]) -> Result<Option<Vec<u8>>> {
        let mut iter = self.connection.iter_for(self.duration);
        self.send(request)?;
        self.receive(&mut iter)
    }

    fn transport_send(&self, request: &[u8]) -> Result<()> {
        // send first frame
        let size = request.len();
        let payload = [&(0x1000 | (size as u16)).to_be_bytes(), &request[0..6]].concat();
        let first_frame = Packet::new(self.send_header, &payload);
        let mut flow_control_stream = self.connection.iter_for(Duration::from_secs(2));
        self.connection.send(&first_frame)?;

        // response to flow control
        let flow_control = flow_control_stream.find(|p| p.id & 0xFFFFFF == self.receive_header);
        match flow_control {
            Some(p) => {
                if p.payload[0] == 0x7F {
                    Err(anyhow!("NACK: {request:?} -> {p}"))
                } else if p.payload[0] != 0x30 {
                    Err(anyhow!(
                        "Unexpected: {request:?} -> {p} should this be ignored?"
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
                        let end = Ord::min(7 + offset, request.len());
                        let mut payload =
                            [&[0x20 | (sequence as u8 & 0xF)], &request[offset..end]].concat();
                        while payload.len() < 8 {
                            payload.push(0xFF);
                        }
                        let consecutive = Packet::new(self.send_header, &payload);
                        self.connection.send(&consecutive)?;
                    }

                    Ok(())
                }
            }
            None => Err(anyhow!("No response to: {request:X?}",)),
        }
    }

    fn transport_receive(&self, packet: &Packet) -> Result<Option<Vec<u8>>> {
        let stream = self.connection.iter_for(self.duration);

        // send flow control
        let payload = [0x30, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let flow_control = Packet::new(self.send_header, &payload);
        self.connection.send(&flow_control)?;

        let mut result = Vec::new();
        // collect payload from first packet
        result.extend(packet.payload[2..].iter());

        // collect all payload from the rest
        let bytes: [u8; 2] = packet.payload[0..2]
            .try_into()
            .expect("Failed to parse length.");
        let len = u16::from_be_bytes(bytes) & 0x0FFF;
        let frames = len / 7;
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
    use super::*;
    use crate::sim::SimulatedConnection;
    use anyhow::Ok;
    #[test]
    fn send8() -> Result<()> {
        let connection = SimulatedConnection::new()?;
        let mut stream = connection.iter_for(Duration::from_secs(2));
        let tp = Iso15765::new(&connection, 0xDA00, Duration::from_secs(2), 0xF9, 0);
        tp.send(&[1, 2, 3])?;
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
        let connection = SimulatedConnection::new()?;

        let log = connection.iter();
        thread::spawn(move || log.filter(|p| p.is_some()).for_each(|p| eprintln!("{p:?}")));

        let tx_connection = connection.clone();
        let mut stream = tx_connection.iter_for(Duration::from_secs(2));
        thread::spawn(move || {
            let mut tp = Iso15765::new(&tx_connection, 0xDA00, Duration::from_secs(2), 0, 0xF9);
            let rx = tp.receive(&mut stream).unwrap().unwrap();
            eprintln!(" rx: {rx:?}");
            let tx = rx.iter().map(|u| u + 3).collect::<Vec<u8>>();
            eprintln!(" tx: {tx:?}");

            tp.send(&tx).expect("Failed to send");
        });

        let mut tp = Iso15765::new(&connection, 0xDA00, Duration::from_secs(2), 0xF9, 0);
        let buf = tp.send_receive(&[1, 2, 3])?;
        assert_eq!(vec![0x04u8, 0x05, 0x06], buf.unwrap());
        Ok(())
    }
    #[test]
    fn send14() -> Result<()> {
        const DURATION: Duration = Duration::from_secs(2);
        let rx_connection = SimulatedConnection::new()?;
        let tx_connection = rx_connection.clone();

        let mut stream = rx_connection.iter_for(DURATION);

        thread::spawn(move || {
            let tx_tp = Iso15765::new(&tx_connection, 0xDA00, DURATION, 0xF9, 0);
            tx_tp.send(&[0x55; 14]).expect("Failed to send");
        });

        let mut rx_tp = Iso15765::new(&rx_connection, 0xDA00, DURATION, 0, 0xF9);
        let packet = rx_tp.receive(&mut stream)?;

        assert_eq!([0x55; 14][..], packet.unwrap());
        Ok(())
    }
    #[test]
    fn send4000() -> Result<()> {
        const DURATION: Duration = Duration::from_secs(2);
        let rx_connection = SimulatedConnection::new()?;
        let tx_connection = rx_connection.clone();

        let mut stream = rx_connection.iter_for(DURATION);

        thread::spawn(move || {
            let tx_tp = Iso15765::new(&tx_connection, 0xDA00, DURATION, 0xF9, 0);
            tx_tp.send(&[0x55; 4000]).expect("Failed to send");
        });

        let rx_tp = Iso15765::new(&rx_connection, 0xDA00, DURATION, 0, 0xF9);
        let packet = rx_tp.receive(&mut stream)?.unwrap();
        assert_eq!(4000, packet.len());
        assert_eq!([0x55; 4000][..], packet);
        Ok(())
    }

    struct SimVin {
        vin: String,
        session: u8,
    }
    impl SimVin {
        pub fn run(mut self, connection: &dyn Connection) -> Result<()> {
            let uds = Iso15765::new(connection, 0xDA00, Duration::from_secs(2), 0x03, 0xF9);
            let mut iter = connection.iter().flatten();
            loop {
                let buf = uds.receive(&mut iter)?.unwrap();
                eprintln!("sim rx: {buf:X?}");
                iter = connection.iter().flatten();
                let response = match buf[0] {
                    0x10 => {
                        self.session = buf[1];
                        vec![0x50, self.session]
                    }
                    0x22 => {
                        let did =
                            u16::from_be_bytes(buf[1..3].try_into().expect("Unable to parse DID"));
                        if did == 0xf190 {
                            [&[0x62, 0xF1, 0x90], self.vin.as_bytes()].concat()
                        } else {
                            vec![0x7F, 0x22, 0x20]
                        }
                    }
                    0x2E => {
                        let did =
                            u16::from_be_bytes(buf[1..3].try_into().expect("Unable to parse DID"));
                        if did == 0xf190 && self.session == 3 {
                            self.vin =
                                String::from_utf8(buf[3..].to_vec()).expect("Unable to set VIN");
                            vec![0x6E, 0xF1, 0x90]
                        } else {
                            vec![0x7F, 0x22, if self.session == 3 { 0x20 } else { 0x32 }]
                        }
                    }
                    _default => panic!("Unknown command"),
                };
                eprintln!("sim tx: {response:X?}");
                uds.send(&response)?;
            }
        }
    }
    #[test]
    fn example() -> Result<()> {
        let connection = SimulatedConnection::new()?;
        let sim_connection = connection.clone();

        // let log = connection.iter_for(Duration::from_secs(9999));
        // thread::spawn(move || log.for_each(|p| eprintln!("{p}")));

        thread::spawn(move || {
            let _run = SimVin {
                vin: "12345678901234567".into(),
                session: 1,
            }
            .run(&sim_connection);
        });

        thread::sleep(Duration::from_millis(50));

        let uds = Iso15765::new(&connection, 0xDA00, Duration::from_secs(2), 0xF9, 0x03);

        eprintln!("read VIN");
        assert_eq!(
            "12345678901234567".as_bytes(),
            &uds.send_receive(&[0x22, 0xf1, 0x90])?.unwrap()[3..]
        );

        eprintln!("session 3");
        uds.send_receive(&[0x10, 0x03])?;

        // auth
        // skipping this typical step

        eprintln!("write VIN");
        uds.send_receive(&[&[0x2E, 0xF1, 0x90], "TEST VIN".as_bytes()].concat())?;

        eprintln!("session 1");
        uds.send_receive(&[0x10, 0x01])?;

        eprintln!("read VIN");
        assert_eq!(
            "TEST VIN".as_bytes(),
            &uds.send_receive(&[0x22, 0xf1, 0x90])?.unwrap()[3..]
        );

        // fail to write VIN
        assert_eq!(
            0x7F,
            uds.send_receive(&[&[0x2E, 0xF1, 0x90], "TEST VIN".as_bytes()].concat())?
                .unwrap()[0]
        );

        Ok(())
    }
}
