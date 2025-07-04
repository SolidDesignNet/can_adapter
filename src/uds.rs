use crate::{uds::iso15765::Iso15765, CanContext};
use anyhow::Result;
use clap::*;
use clap_num::maybe_hex;
use std::time::Duration;

mod iso15765;

#[derive(Subcommand, Debug, Clone)]
pub enum Uds {
    #[command(name = "sessionControl")]
    S10 {
        #[arg(value_parser=maybe_hex::<u8>)]
        session: u8,
    },
    #[command(name = "readDataByIdentifier")]
    S22 {
        #[arg(value_parser=maybe_hex::<u16>)]
        did: u16,
    },
    #[command(name = "writeDataByIdentifier")]
    S2E {
        #[arg(value_parser=maybe_hex::<u16>)]
        did: u16,
        #[clap(value_parser=crate::hex_array)]
        value: Box<[u8]>,
    },
    #[command(name = "ioController")]
    S2F {
        #[arg(value_parser=maybe_hex::<u16>)]
        did: u16,
        #[clap(value_parser=crate::hex_array)]
        value: Box<[u8]>,
    },
    #[command(name = "auth")]
    S27 {
        #[arg(value_parser=maybe_hex::<u8>)]
        seed_id: u8,
        #[arg(value_parser=maybe_hex::<u8>)]
        key_id: u8,
        #[clap(value_parser=crate::hex_array)]
        key: Box<[u8]>,
    },
}
impl Uds {
    pub fn execute(&self, can_can: &mut CanContext) -> Result<Option<UdsBuffer>> {
        match self {
            Uds::S10 { session } => Iso14229Command::build(can_can.can_can.timeout(), 0x10)
                .u8(&[*session])
                .execute_report(can_can),
            Uds::S22 { did } => Iso14229Command::build(can_can.can_can.timeout(), 0x22)
                .u16(&[*did])
                .execute_report(can_can),
            Uds::S2E { did, value } => todo!(),
            Uds::S2F { did, value } => todo!(),
            Uds::S27 {
                seed_id,
                key_id,
                key,
            } => todo!(),
        }
    }
}

type UdsBuffer = Vec<u8>;

struct Iso14229Command {
    raw: Vec<u8>,
    pgn: u32,
    duration: Duration,
}
impl Default for Iso14229Command {
    fn default() -> Self {
        Self {
            raw: Default::default(),
            pgn: 0xDA00,
            duration: Duration::from_secs(2),
        }
    }
}
impl Iso14229Command {
    pub fn build(duration: Duration, command: u8) -> Iso14229Command {
        let mut new = Iso14229Command {
            raw: Default::default(),
            pgn: 0xDA00,
            duration,
        };
        new.u8(&[command]);
        new
    }
    pub fn u8(&mut self, data: &[u8]) -> &mut Self {
        for d in data {
            self.raw.push(*d);
        }
        self
    }
    pub fn u16(&mut self, data: &[u16]) -> &mut Self {
        for d in data {
            self.raw.push((*d >> 8) as u8);
            self.raw.push(*d as u8);
        }
        self
    }
    pub fn u24(&mut self, data: &[u32]) -> &mut Self {
        for d in data {
            self.raw.push((*d >> 16) as u8);
            self.raw.push((*d >> 8) as u8);
            self.raw.push(*d as u8);
        }
        self
    }
    pub fn u32(&mut self, data: &[u32]) -> &mut Self {
        for d in data {
            self.raw.push((*d >> 24) as u8);
            self.raw.push((*d >> 16) as u8);
            self.raw.push((*d >> 8) as u8);
            self.raw.push(*d as u8);
        }
        self
    }
    pub fn u64(&mut self, data: &[u64]) -> &mut Self {
        for d in data {
            let d = *d;
            for i in (0..7).rev() {
                self.raw.push((d >> (i * 8)) as u8);
            }
        }
        self
    }
    pub fn execute_report(&self, can_can: &mut CanContext) -> Result<Option<UdsBuffer>> {
        let r = self.execute(can_can);
        eprintln!("sent     {:X?}", self.raw);
        eprintln!("received {r:X?}");
        r
    }

    // Err(None) means no response.
    /// Err(UdsBuffer) is the NACK
    pub fn execute(&self, can_can: &mut CanContext) -> Result<Option<UdsBuffer>> {
        let connection = can_can.connection.as_mut();
        let mut result = connection.iter_for(self.duration);
        let mut iso15765 = Iso15765::new(
            connection,
            self.pgn,
            self.duration,
            can_can.can_can.source_address,
            can_can.can_can.destination_address,
        );
        iso15765.send(&self.raw)?;
        iso15765.receive(&mut result)
    }
}
