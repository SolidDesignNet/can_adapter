use anyhow::Result;

use crate::connection::Connection;
use clap::Subcommand;
#[derive(Subcommand, Debug, Clone)]
pub enum Uds {
    #[command(name = "sessionControl")]
    S10 { session: u8 },
    #[command(name = "readDataByIdentifier")]
    S22 { did: u16 },
    #[command(name = "writeDataByIdentifier")]
    S2e {
        did: u16,
        #[clap(value_parser=crate::hex_array)]
        value: Box<[u8]>,
    },
    #[command(name = "ioController")]
    S2f { did: u16 },
    #[command(name = "Auth")]
    S27 {},
}

impl Uds {
    pub fn execute(&self, cf: &dyn Connection) -> Result<()> {
        Ok(())
    }
}
