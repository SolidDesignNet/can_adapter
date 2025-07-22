use std::fmt::*;

/// Packets can be sent on a connection.
///
#[derive(Debug, Clone)]
pub struct Packet {
    pub id: u32,
    pub payload: Vec<u8>,
    pub tx: bool,
    pub time: f64,
}
impl Display for Packet {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        todo!()
    }
}
impl Packet {
    pub fn data_str_nospace(&self) -> String {
        as_hex_nospace(&self.payload)
    }
}
fn as_hex_nospace(data: &[u8]) -> String {
    // FIXME optimize
    let mut s = String::new();
    for byte in data {
        write!(&mut s, "{byte:02X}").expect("Unable to write");
    }
    s
}
