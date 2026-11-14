//! CRC-32 (IEEE 802.3), used for GPT-free verification of copied data.
pub struct Crc32 {
    table: [u32; 256],
    value: u32,
}

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

impl Crc32 {
    pub fn new() -> Self {
        let mut table = [0u32; 256];
        for (i, t) in table.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            *t = c;
        }
        Crc32 { table, value: 0xFFFF_FFFF }
    }
    pub fn update(&mut self, data: &[u8]) {
        let mut c = self.value;
        for &b in data {
            c = self.table[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
        }
        self.value = c;
    }
    pub fn finish(&self) -> u32 {
        self.value ^ 0xFFFF_FFFF
    }
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut c = Crc32::new();
    c.update(data);
    c.finish()
}

#[cfg(test)]
mod t {
    #[test]
    fn known() {
        assert_eq!(super::crc32(b"123456789"), 0xCBF4_3926);
    }
}
