//! Helpers for the in-app updater: version comparison and SHA-256.
//! (Network access lives in the app; this part is pure and unit-tested.)

/// Parses "v0.0.3", "0.0.3", "0.0.4-test.2" into ([major, minor, patch], pre-release).
pub fn parse_version(s: &str) -> Option<([u64; 3], Option<String>)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let (core, pre) = match s.split_once('-') {
        Some((c, p)) => (c, Some(p.to_string())),
        None => (s, None),
    };
    let mut parts = core.split('.');
    let mut v = [0u64; 3];
    for x in v.iter_mut() {
        *x = parts.next()?.trim().parse().ok()?;
    }
    Some((v, pre))
}

/// Pre-release ("test.2") ranks below the plain release of the same number.
fn pre_key(p: &Option<String>) -> (u8, Vec<u64>, String) {
    match p {
        None => (1, vec![], String::new()),
        Some(s) => (0, s.split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse().ok()).collect(), s.clone()),
    }
}

/// true if `remote` is a newer version than `local`.
pub fn is_newer(remote: &str, local: &str) -> bool {
    match (parse_version(remote), parse_version(local)) {
        (Some((rv, rp)), Some((lv, lp))) => (rv, pre_key(&rp)) > (lv, pre_key(&lp)),
        _ => false,
    }
}

/// Readable version for the UI: "v0.0.4 test 2".
pub fn display_version(v: &str) -> String {
    match parse_version(v) {
        Some((n, None)) => format!("v{}.{}.{}", n[0], n[1], n[2]),
        Some((n, Some(p))) => format!("v{}.{}.{} {}", n[0], n[1], n[2], p.replace(['.', '-'], " ")),
        None => v.to_string(),
    }
}

// ------------------------------------------------------------------ SHA-256

pub struct Sha256 {
    h: [u32; 8],
    buf: Vec<u8>,
    len: u64,
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    pub fn new() -> Self {
        Sha256 { h: [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19], buf: Vec::with_capacity(64), len: 0 }
    }
    fn block(&mut self, b: &[u8]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = self.h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for (h, x) in self.h.iter_mut().zip(v) {
            *h = h.wrapping_add(x);
        }
    }
    pub fn update(&mut self, mut data: &[u8]) {
        self.len += data.len() as u64;
        if !self.buf.is_empty() {
            let take = (64 - self.buf.len()).min(data.len());
            self.buf.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.buf.len() == 64 {
                let b = std::mem::take(&mut self.buf);
                self.block(&b);
            }
        }
        while data.len() >= 64 {
            let (b, rest) = data.split_at(64);
            self.block(b);
            data = rest;
        }
        self.buf.extend_from_slice(data);
    }
    pub fn hex(mut self) -> String {
        let bits = self.len * 8;
        let mut pad = vec![0x80u8];
        while (self.buf.len() + pad.len()) % 64 != 56 {
            pad.push(0);
        }
        pad.extend_from_slice(&bits.to_be_bytes());
        let len_before = self.len;
        self.update(&pad);
        self.len = len_before;
        self.h.iter().map(|x| format!("{x:08x}")).collect()
    }
}

pub fn sha256_file(path: &std::path::Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut s = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        s.update(&buf[..n]);
    }
    Ok(s.hex())
}

/// Finds the checksum for `file` in a SHA256SUMS text ("<hex>  <name>").
pub fn sum_for(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let h = it.next()?;
        let n = it.next()?.trim_start_matches('*');
        (n == file && h.len() == 64).then(|| h.to_ascii_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sha() {
        assert_eq!(Sha256::new().hex(), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        let mut s = Sha256::new();
        s.update(b"abc");
        assert_eq!(s.hex(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let mut s = Sha256::new();
        let data = vec![b'a'; 1_000_000];
        for c in data.chunks(777) {
            s.update(c);
        }
        assert_eq!(s.hex(), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
    }
    #[test]
    fn update_decision() {
        assert!(is_newer("v0.0.4", "0.0.3")); // newer
        assert!(!is_newer("v0.0.3", "0.0.3")); // same
        assert!(!is_newer("v0.0.2", "0.0.3")); // older
        assert!(is_newer("v0.1.0", "0.0.9"));
        assert!(is_newer("v0.0.4-test.1", "0.0.3")); // test build on top of a release
        assert!(!is_newer("v0.0.4-test.1", "0.0.4")); // release beats its test build
        assert!(is_newer("v0.0.4-test.2", "0.0.4-test.1"));
        assert!(is_newer("v0.0.4", "0.0.4-test.2"));
        assert!(!is_newer("garbage", "0.0.3"));
        assert_eq!(display_version("0.0.4-test.2"), "v0.0.4 test 2");
    }
    #[test]
    fn sums() {
        let t = "aa  x\n0123456789012345678901234567890123456789012345678901234567890123  Windows-ISO-Editor_0.0.4_x86_64.AppImage\n";
        assert!(sum_for(t, "Windows-ISO-Editor_0.0.4_x86_64.AppImage").is_some());
        assert!(sum_for(t, "x").is_none());
    }
}
