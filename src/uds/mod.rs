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
        id: u8,
        #[clap(value_parser=crate::hex_array)]
        key: Box<[u8]>,
    },
}
impl Uds {
    pub fn execute(&self, context: &mut CanContext) -> Result<Option<Vec<u8>>> {
        self.cmd(context).execute(context)
    }

    pub fn execute_and_report(&self, context: &mut CanContext) -> Result<Option<Vec<u8>>> {
        let cmd = self.cmd(context);
        let r = cmd.execute(context);
        eprintln!("sent     {:X?}", cmd.raw);
        if let Ok(Some(b)) = &r {
            eprintln!("received {b:X?}");
        } else {
            eprintln!("invalid response {r:X?}");
        };
        r
    }

    pub fn cmd(&self, context: &mut CanContext) -> Iso14229Command {
        match self {
            Uds::S10 { session } => {
                Iso14229Command::build(context.can_can.timeout(), 0x10).u8(&[*session])
            }
            Uds::S22 { did } => {
                Iso14229Command::build(context.can_can.timeout(), 0x22).u16(&[*did])
            }
            Uds::S2E { did, value } => Iso14229Command::build(context.can_can.timeout(), 0x2E)
                .u16(&[*did])
                .u8(value),
            Uds::S2F { did, value } => Iso14229Command::build(context.can_can.timeout(), 0x2F)
                .u16(&[*did])
                .u8(value),
            Uds::S27 { id, key } => Iso14229Command::build(context.can_can.timeout(), 0x27)
                .u8(&[*id])
                .u8(key),
        }
    }
}

pub struct Iso14229Command {
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
        Iso14229Command {
            raw: Default::default(),
            pgn: 0xDA00,
            duration,
        }
        .u8(&[command])
    }
    pub fn u8(mut self, data: &[u8]) -> Self {
        for d in data {
            self.raw.push(*d);
        }
        self
    }
    pub fn u16(mut self, data: &[u16]) -> Self {
        for d in data {
            self.raw.push((*d >> 8) as u8);
            self.raw.push(*d as u8);
        }
        self
    }
    pub fn u24(mut self, data: &[u32]) -> Self {
        for d in data {
            self.raw.push((*d >> 16) as u8);
            self.raw.push((*d >> 8) as u8);
            self.raw.push(*d as u8);
        }
        self
    }
    pub fn u32(mut self, data: &[u32]) -> Self {
        for d in data {
            self.raw.push((*d >> 24) as u8);
            self.raw.push((*d >> 16) as u8);
            self.raw.push((*d >> 8) as u8);
            self.raw.push(*d as u8);
        }
        self
    }
    pub fn u64(mut self, data: &[u64]) -> Self {
        for d in data {
            let d = *d;
            for i in (0..7).rev() {
                self.raw.push((d >> (i * 8)) as u8);
            }
        }
        self
    }

    // Err(None) means no response.
    /// Err(UdsBuffer) is the NACK
    pub fn execute(&self, context: &mut CanContext) -> Result<Option<Vec<u8>>> {
        let connection = context.connection.as_mut();
        let iso15765 = Iso15765::new(
            connection,
            self.pgn,
            self.duration,
            context.can_can.source_address,
            context.can_can.destination_address,
        );

        iso15765.send_receive(&self.raw)
    }
}
