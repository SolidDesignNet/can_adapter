use zerocopy::{Immutable, IntoBytes, TryFromBytes};


#[repr(C, packed)]
#[derive(Immutable, IntoBytes, TryFromBytes)]
pub struct Pgn {
    value: [u8; 3],
}

impl From<u32> for Pgn {
    fn from(v: u32) -> Self {
        let mut value = [0u8; 3];
        value.copy_from_slice(&v.as_bytes()[0..3]);
        Pgn { value }
    }
}
impl From<Pgn> for u32 {
    fn from(v: Pgn) -> Self {
        u32::from_be_bytes([0, v.value[0], v.value[1], v.value[2]])
    }
}